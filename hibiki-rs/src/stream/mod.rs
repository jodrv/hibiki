// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::Result;
use candle::Device;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

mod devices;
mod input;
mod model;
mod playback;
mod resampler;
mod wav_writer;

pub use devices::list_devices;

pub struct StreamConfig {
    // Input source (exactly one)
    pub input_file: Option<PathBuf>,
    pub input_device: Option<String>,
    
    // Output routing
    pub output_device: Option<String>,
    pub disable_speaker: bool,
    
    // WAV saving
    pub save_output: Option<PathBuf>,
    
    // Model config
    pub lm_config: moshi::lm::Config,
    pub lm_model_file: PathBuf,
    pub mimi_model_file: PathBuf,
    pub text_tokenizer: PathBuf,
    pub seed: u64,
    pub cfg_alpha: Option<f64>,
}

struct Metrics {
    frames_captured: Arc<AtomicU64>,
    frames_processed: Arc<AtomicU64>,
    last_log_time: Instant,
}

use std::sync::atomic::AtomicU64;

impl Metrics {
    fn new() -> Self {
        Self {
            frames_captured: Arc::new(AtomicU64::new(0)),
            frames_processed: Arc::new(AtomicU64::new(0)),
            last_log_time: Instant::now(),
        }
    }
    
    fn should_log(&mut self) -> bool {
        if self.last_log_time.elapsed() >= Duration::from_secs(5) {
            self.last_log_time = Instant::now();
            true
        } else {
            false
        }
    }
}

pub fn run(config: StreamConfig, device: &Device) -> Result<()> {
    // Validate input
    match (&config.input_file, &config.input_device) {
        (Some(_), Some(_)) => anyhow::bail!("Specify either --input-file or --input-device, not both"),
        (None, None) => anyhow::bail!("Must specify either --input-file or --input-device"),
        _ => {}
    }
    
    // Log configuration
    tracing::info!("=== Hibiki Streaming Configuration ===");
    if let Some(ref path) = config.input_file {
        tracing::info!("Input: File '{}'", path.display());
    } else if let Some(ref dev) = config.input_device {
        tracing::info!("Input: Microphone '{}'", dev);
    }
    
    if config.disable_speaker {
        tracing::info!("Output: Speaker disabled");
    } else if let Some(ref dev) = config.output_device {
        tracing::info!("Output: Speaker '{}'", dev);
    } else {
        tracing::info!("Output: Default speaker");
    }
    
    if let Some(ref path) = config.save_output {
        tracing::info!("Save to: {}", path.display());
    } else {
        tracing::info!("Save to: (none)");
    }
    
    // Setup shutdown signal
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = shutdown.clone();
    ctrlc::set_handler(move || {
        tracing::info!("Shutdown signal received");
        shutdown_ctrlc.store(true, Ordering::Relaxed);
    })?;
    
    // Create channels
    let (capture_tx, capture_rx) = mpsc::sync_channel::<[f32; resampler::FRAME_SIZE]>(50);
    let (audio_tx, audio_rx) = mpsc::sync_channel::<Vec<f32>>(50);
    let (text_tx, text_rx) = mpsc::channel::<String>();
    
    // Start capture thread
    let shutdown_capture = shutdown.clone();
    let capture_handle = if let Some(ref path) = config.input_file {
        let path = path.clone();
        thread::Builder::new()
            .name("capture-file".to_string())
            .spawn(move || input::run_file_input(&path, capture_tx, shutdown_capture))?
    } else if let Some(ref dev_name) = config.input_device {
        let device = devices::find_input_device(dev_name)?;
        thread::Builder::new()
            .name("capture-mic".to_string())
            .spawn(move || input::run_mic_input(device, capture_tx, shutdown_capture))?
    } else {
        unreachable!()
    };
    
    // Setup audio routing based on configuration
    let (playback_handle, wav_handle) = if config.save_output.is_some() && !config.disable_speaker {
        // Both playback and WAV: need to tee the audio
        let (playback_tx, playback_rx) = mpsc::sync_channel::<Vec<f32>>(50);
        let (wav_tx, wav_rx) = mpsc::sync_channel::<Vec<f32>>(50);
        
        // Tee thread: receives from model, sends to both playback and WAV
        thread::Builder::new()
            .name("audio-tee".to_string())
            .spawn(move || {
                while let Ok(samples) = audio_rx.recv() {
                    let _ = playback_tx.send(samples.clone());
                    let _ = wav_tx.send(samples);
                }
            })?;
        
        // Playback thread
        let device = devices::find_output_device(config.output_device.as_deref())?;
        let shutdown_playback = shutdown.clone();
        let playback_h = thread::Builder::new()
            .name("playback".to_string())
            .spawn(move || {
                let mut sink = match playback::SpeakerSink::new(device) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to create speaker sink: {}", e);
                        return (0, 0, 0);
                    }
                };
                
                loop {
                    match playback_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(samples) => {
                            if let Err(e) = sink.push_samples(&samples) {
                                tracing::error!("Playback error: {}", e);
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Check if we should exit (only after channel closed)
                            if shutdown_playback.load(Ordering::Relaxed) {
                                tracing::info!("Playback thread: shutdown requested, {} samples in buffer", sink.buffer_level());
                                break;
                            }
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            tracing::info!("Input ended, draining {} samples from buffer...", sink.buffer_level());
                            break;
                        }
                    }
                }
                
                // CRITICAL: Wait for buffered audio to finish playing
                let buffer_level = sink.buffer_level();
                if buffer_level > 0 {
                    let drain_seconds = buffer_level as f64 / 24000.0;
                    tracing::info!("Waiting {:.1}s for remaining audio to play out...", drain_seconds);
                    thread::sleep(Duration::from_secs_f64(drain_seconds + 0.5)); // +0.5s safety margin
                }
                
                (sink.underrun_count(), sink.overflow_count(), sink.buffer_level())
            })?;
        
        // WAV writer thread
        let path = config.save_output.as_ref().unwrap().clone();
        let wav_h = thread::Builder::new()
            .name("wav-writer".to_string())
            .spawn(move || wav_writer::run_wav_writer(&path, wav_rx))?;
        
        (Some(playback_h), Some(wav_h))
    } else if !config.disable_speaker {
        // Playback only, no WAV
        let device = devices::find_output_device(config.output_device.as_deref())?;
        let shutdown_playback = shutdown.clone();
        
        let playback_h = thread::Builder::new()
            .name("playback".to_string())
            .spawn(move || {
                let mut sink = match playback::SpeakerSink::new(device) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to create speaker sink: {}", e);
                        return (0, 0, 0);
                    }
                };
                
                loop {
                    match audio_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(samples) => {
                            if let Err(e) = sink.push_samples(&samples) {
                                tracing::error!("Playback error: {}", e);
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Check if we should exit (only after channel closed)
                            if shutdown_playback.load(Ordering::Relaxed) {
                                tracing::info!("Playback thread: shutdown requested, {} samples in buffer", sink.buffer_level());
                                break;
                            }
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            tracing::info!("Input ended, draining {} samples from buffer...", sink.buffer_level());
                            break;
                        }
                    }
                }
                
                // CRITICAL: Wait for buffered audio to finish playing
                let buffer_level = sink.buffer_level();
                if buffer_level > 0 {
                    let drain_seconds = buffer_level as f64 / 24000.0;
                    tracing::info!("Waiting {:.1}s for remaining audio to play out...", drain_seconds);
                    thread::sleep(Duration::from_secs_f64(drain_seconds + 0.5)); // +0.5s safety margin
                }
                
                (sink.underrun_count(), sink.overflow_count(), sink.buffer_level())
            })?;
        
        (Some(playback_h), None)
    } else if let Some(ref path) = config.save_output {
        // WAV only, no playback
        let path = path.clone();
        let wav_h = thread::Builder::new()
            .name("wav-writer".to_string())
            .spawn(move || wav_writer::run_wav_writer(&path, audio_rx))?;
        
        (None, Some(wav_h))
    } else {
        // Neither playback nor WAV - just drain
        thread::Builder::new()
            .name("audio-drain".to_string())
            .spawn(move || {
                while audio_rx.recv().is_ok() {}
            })?;
        
        (None, None)
    };
    
    // Start text printer thread
    let text_handle = thread::Builder::new()
        .name("text-printer".to_string())
        .spawn(move || {
            use std::io::Write;
            while let Ok(text) = text_rx.recv() {
                print!("{}", text);
                std::io::stdout().flush().unwrap();
            }
            println!(); // Final newline
        })?;
    
    // Load and run model
    tracing::info!("Loading models...");
    let model = model::StreamingModel::new(
        &config.lm_config,
        &config.lm_model_file,
        &config.mimi_model_file,
        &config.text_tokenizer,
        config.seed,
        config.cfg_alpha,
        device,
    )?;
    
    tracing::info!("Starting inference...");
    let shutdown_model = shutdown.clone();
    let model_handle = thread::Builder::new()
        .name("model".to_string())
        .spawn(move || {
            model::run_model_thread(model, capture_rx, audio_tx, text_tx, shutdown_model)
        })?;
    
    // Monitoring loop
    let mut metrics = Metrics::new();
    let mut last_underruns = 0u64;
    let mut last_overflows = 0u64;
    
    while !shutdown.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(500));
        
        if metrics.should_log() {
            if let Some(ref handle) = playback_handle {
                // Note: We can't easily get live stats without more complex IPC
                // For now, just log that we're running
                tracing::info!("Streaming active...");
            }
        }
    }
    
    // Shutdown sequence
    tracing::info!("Shutting down...");
    
    // Wait for capture to finish
    if let Err(e) = capture_handle.join() {
        tracing::error!("Capture thread panicked: {:?}", e);
    }
    
    // Wait for model to finish
    let model_stats = match model_handle.join() {
        Ok(Ok(stats)) => Some(stats),
        Ok(Err(e)) => {
            tracing::error!("Model thread error: {}", e);
            None
        }
        Err(e) => {
            tracing::error!("Model thread panicked: {:?}", e);
            None
        }
    };
    
    // Wait for playback
    if let Some(handle) = playback_handle {
        match handle.join() {
            Ok((underruns, overflows, buffer_level)) => {
                tracing::info!(
                    "Playback stats: {} underruns, {} overflows, {} samples in buffer at shutdown",
                    underruns,
                    overflows,
                    buffer_level
                );
            }
            Err(e) => {
                tracing::error!("Playback thread panicked: {:?}", e);
            }
        }
    }
    
    // Wait for WAV writer
    if let Some(handle) = wav_handle {
        if let Err(e) = handle.join() {
            tracing::error!("WAV writer thread error: {:?}", e);
        }
    }
    
    // Wait for text printer
    if let Err(e) = text_handle.join() {
        tracing::error!("Text printer thread panicked: {:?}", e);
    }
    
    // Print final stats
    if let Some(stats) = model_stats {
        tracing::info!(
            "Model stats: {} frames processed, avg {:.1}ms/frame, p95 {:.1}ms/frame",
            stats.frames_processed,
            stats.avg_time_ms,
            stats.p95_time_ms
        );
    }
    
    tracing::info!("Streaming complete");
    Ok(())
}
