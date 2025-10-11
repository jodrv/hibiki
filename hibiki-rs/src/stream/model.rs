// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::Result;
use candle::{Device, IndexOp, Tensor};
use std::sync::mpsc;
use std::time::Instant;

use super::resampler::FRAME_SIZE;

pub struct StreamingModel {
    mimi: moshi::mimi::Mimi,
    state: moshi::lm_generate_multistream::State,
    text_tokenizer: sentencepiece::SentencePieceProcessor,
    text_start_token: u32,
    prev_text_token: u32,
    generated_audio_codebooks: usize,
    device: Device,
    frame_times: Vec<f32>,
    conditions: Option<moshi::conditioner::Condition>,
}

impl StreamingModel {
    pub fn new(
        lm_config: &moshi::lm::Config,
        lm_model_file: &std::path::Path,
        mimi_model_file: &std::path::Path,
        text_tokenizer_file: &std::path::Path,
        seed: u64,
        cfg_alpha: Option<f64>,
        device: &Device,
    ) -> Result<Self> {
        let dtype = device.bf16_default_to_f32();
        
        tracing::info!("Loading language model...");
        let lm_model = moshi::lm::load_lm_model(lm_config.clone(), lm_model_file, dtype, device)?;
        
        tracing::info!("Loading audio tokenizer (mimi)...");
        let mimi = moshi::mimi::load(
            mimi_model_file.to_str().unwrap(),
            Some(lm_model.generated_audio_codebooks()),
            device,
        )?;
        
        tracing::info!("Loading text tokenizer...");
        let text_tokenizer = sentencepiece::SentencePieceProcessor::open(text_tokenizer_file)?;
        
        let audio_lp = candle_transformers::generation::LogitsProcessor::from_sampling(
            seed,
            candle_transformers::generation::Sampling::TopK { k: 250, temperature: 0.8 },
        );
        let text_lp = candle_transformers::generation::LogitsProcessor::from_sampling(
            seed,
            candle_transformers::generation::Sampling::TopK { k: 25, temperature: 0.8 },
        );
        
        let generated_audio_codebooks = lm_config.depformer.as_ref().map_or(8, |v| v.num_slices);
        
        let conditions = match lm_model.condition_provider() {
            None => None,
            Some(cp) => {
                let cond = if cfg_alpha.is_some() {
                    use moshi::conditioner::Condition::AddToInput;
                    let AddToInput(c1) = cp.condition_lut("description", "very_good")?;
                    let AddToInput(c2) = cp.condition_lut("description", "very_bad")?;
                    AddToInput(Tensor::cat(&[c1, c2], 0)?)
                } else {
                    cp.condition_lut("description", "very_good")?
                };
                Some(cond)
            }
        };
        
        let cfg_alpha = if cfg_alpha == Some(1.) { None } else { cfg_alpha };
        let text_start_token = lm_config.text_out_vocab_size as u32;
        
        let config = moshi::lm_generate_multistream::Config {
            acoustic_delay: 2,
            audio_vocab_size: lm_config.audio_vocab_size,
            generated_audio_codebooks,
            input_audio_codebooks: lm_config.audio_codebooks - generated_audio_codebooks,
            text_start_token,
            text_eop_token: 0,
            text_pad_token: 3,
        };
        
        let state = moshi::lm_generate_multistream::State::new(
            lm_model,
            2500, // max steps
            audio_lp,
            text_lp,
            None,
            None,
            cfg_alpha,
            config,
        );
        
        tracing::info!("Models loaded successfully");
        
        Ok(Self {
            mimi,
            state,
            text_tokenizer,
            text_start_token,
            prev_text_token: text_start_token,
            generated_audio_codebooks,
            device: device.clone(),
            frame_times: Vec::new(),
            conditions,
        })
    }
    
    /// Process one 80ms frame (1920 samples) and return generated audio + text
    pub fn process_frame(&mut self, pcm: &[f32; FRAME_SIZE]) -> Result<(Vec<f32>, Option<String>)> {
        let start = Instant::now();
        
        let in_pcm = Tensor::from_vec(
            pcm.to_vec(),
            (1, 1, FRAME_SIZE),
            &self.device,
        )?;
        
        let mut out_pcm = Vec::new();
        let mut text_output = None;
        
        // Encode input with mimi
        let codes = self.mimi.encode_step(&in_pcm.into())?;
        
        if let Some(codes) = codes.as_option() {
            let (_b, _codebooks, steps) = codes.dims3()?;
            
            for step in 0..steps {
                let codes_step = codes.i((.., .., step..step + 1))?;
                let codes_vec = codes_step.i((0, .., 0))?.to_vec1::<u32>()?;
                
                // Step through LM
                let text_token = self.state.step_(
                    Some(self.prev_text_token),
                    &codes_vec,
                    None,
                    None,
                    self.conditions.as_ref(),
                )?;
                
                // Extract text if valid
                if text_token != 0 && text_token != 3 {
                    if let Some(text) = self.decode_text(text_token) {
                        if text_output.is_none() {
                            text_output = Some(text);
                        } else {
                            text_output.as_mut().unwrap().push_str(&text);
                        }
                    }
                }
                self.prev_text_token = text_token;
                
                // Decode generated audio
                if let Some(audio_tokens) = self.state.last_audio_tokens() {
                    let audio_tokens = Tensor::new(
                        &audio_tokens[..self.generated_audio_codebooks],
                        &self.device,
                    )?
                    .reshape((1, 1, ()))?
                    .t()?;
                    
                    let decoded = self.mimi.decode_step(&audio_tokens.into())?;
                    if let Some(decoded) = decoded.as_option() {
                        let decoded_vec = decoded.i((0, 0))?.to_vec1::<f32>()?;
                        out_pcm.extend_from_slice(&decoded_vec);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f32();
        self.frame_times.push(elapsed);
        
        Ok((out_pcm, text_output))
    }
    
    fn decode_text(&self, text_token: u32) -> Option<String> {
        if self.prev_text_token == self.text_start_token {
            self.text_tokenizer.decode_piece_ids(&[text_token]).ok()
        } else {
            let prev_ids = self.text_tokenizer.decode_piece_ids(&[self.prev_text_token]).ok();
            let ids = self.text_tokenizer.decode_piece_ids(&[self.prev_text_token, text_token]).ok();
            prev_ids.and_then(|prev_ids| {
                ids.map(|ids| {
                    if ids.len() > prev_ids.len() {
                        ids[prev_ids.len()..].to_string()
                    } else {
                        String::new()
                    }
                })
            })
        }
    }
    
    pub fn get_stats(&self) -> ModelStats {
        if self.frame_times.is_empty() {
            return ModelStats {
                avg_time_ms: 0.0,
                p95_time_ms: 0.0,
                frames_processed: 0,
            };
        }
        
        let mut sorted = self.frame_times.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
        let p95_idx = (sorted.len() as f32 * 0.95) as usize;
        let p95 = sorted[p95_idx.min(sorted.len() - 1)];
        
        ModelStats {
            avg_time_ms: avg * 1000.0,
            p95_time_ms: p95 * 1000.0,
            frames_processed: sorted.len(),
        }
    }
}

pub struct ModelStats {
    pub avg_time_ms: f32,
    pub p95_time_ms: f32,
    pub frames_processed: usize,
}

/// Run model inference thread
pub fn run_model_thread(
    mut model: StreamingModel,
    input_rx: mpsc::Receiver<[f32; FRAME_SIZE]>,
    audio_tx: mpsc::SyncSender<Vec<f32>>,
    text_tx: mpsc::Sender<String>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<ModelStats> {
    use std::sync::atomic::Ordering;
    
    tracing::info!("Model thread started");
    let mut frames_received = 0u64;
    let mut last_log = std::time::Instant::now();
    
    while !shutdown.load(Ordering::Relaxed) {
        match input_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(frame) => {
                frames_received += 1;
                
                // Calculate RMS of input frame to detect silence
                let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
                
                // Log every 5 seconds to confirm model is receiving input
                if last_log.elapsed().as_secs() >= 5 {
                    tracing::info!("🎤 Model received {} frames so far (latest RMS: {:.4})", frames_received, rms);
                    last_log = std::time::Instant::now();
                }
                
                match model.process_frame(&frame) {
                    Ok((audio, text)) => {
                        if !audio.is_empty() {
                            tracing::debug!("🔊 Model generated {} audio samples", audio.len());
                            let _ = audio_tx.send(audio);
                        }
                        if let Some(text) = text {
                            let _ = text_tx.send(text);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Model processing error: {}", e);
                        shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                tracing::info!("Model thread: input channel closed - signaling shutdown");
                shutdown.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
    
    let stats = model.get_stats();
    tracing::info!("Model thread finished");
    Ok(stats)
}
