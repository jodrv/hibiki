// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::resampler::{StreamingResampler, FRAME_SIZE, TARGET_SAMPLE_RATE};

pub type AudioFrame = [f32; FRAME_SIZE];

/// Reads audio from a file, paces it to wall clock, and emits 80ms frames
pub fn run_file_input<P: AsRef<Path>>(
    path: P,
    tx: mpsc::SyncSender<AudioFrame>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use std::sync::atomic::Ordering;
    
    // Decode entire file
    let (mut pcm, sample_rate) = crate::audio_io::pcm_decode(path)?;
    tracing::info!(
        "File decoded: {} samples at {} Hz",
        pcm.len(),
        sample_rate
    );
    
    // Pad with silence at end
    pcm.extend_from_slice(&vec![0.0; 12000]);
    
    // Create resampler if needed
    let mut resampler = if sample_rate as usize != TARGET_SAMPLE_RATE {
        tracing::info!("Resampling from {} Hz to {} Hz", sample_rate, TARGET_SAMPLE_RATE);
        Some(StreamingResampler::new(sample_rate as usize, 1)?)
    } else {
        None
    };
    
    let frame_duration = Duration::from_millis(80);
    let start_time = Instant::now();
    let mut frame_idx = 0;
    
    if let Some(ref mut resampler) = resampler {
        // Need to resample
        let frames = resampler.push_samples(&pcm)?;
        for frame in frames {
            if shutdown.load(Ordering::Relaxed) {
                tracing::info!("File input shutdown requested");
                return Ok(());
            }
            
            // Pace to wall clock
            let expected_time = start_time + frame_duration * frame_idx;
            let now = Instant::now();
            if now < expected_time {
                std::thread::sleep(expected_time - now);
            }
            
            if tx.send(frame).is_err() {
                tracing::info!("File input: receiver dropped");
                return Ok(());
            }
            
            frame_idx += 1;
        }
        
        // Flush remaining
        if let Some(frame) = resampler.flush()? {
            if !shutdown.load(Ordering::Relaxed) {
                let _ = tx.send(frame);
            }
        }
    } else {
        // No resampling needed, send directly
        for chunk in pcm.chunks(FRAME_SIZE) {
            if shutdown.load(Ordering::Relaxed) {
                tracing::info!("File input shutdown requested");
                return Ok(());
            }
            
            if chunk.len() < FRAME_SIZE {
                // Pad last frame
                let mut frame = [0.0f32; FRAME_SIZE];
                frame[..chunk.len()].copy_from_slice(chunk);
                let _ = tx.send(frame);
                break;
            }
            
            let mut frame = [0.0f32; FRAME_SIZE];
            frame.copy_from_slice(chunk);
            
            // Pace to wall clock
            let expected_time = start_time + frame_duration * frame_idx;
            let now = Instant::now();
            if now < expected_time {
                std::thread::sleep(expected_time - now);
            }
            
            if tx.send(frame).is_err() {
                tracing::info!("File input: receiver dropped");
                return Ok(());
            }
            
            frame_idx += 1;
        }
    }
    
    tracing::info!("File input complete: {} frames", frame_idx);
    Ok(())
}

/// Captures audio from a microphone and emits 80ms frames
pub fn run_mic_input(
    device: cpal::Device,
    tx: mpsc::SyncSender<AudioFrame>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use std::sync::atomic::Ordering;
    
    let config = device.default_input_config()
        .context("Failed to get default input config")?;
    
    tracing::info!(
        "Mic input config: {} channels, {} Hz, {:?}",
        config.channels(),
        config.sample_rate().0,
        config.sample_format()
    );
    
    let sample_rate = config.sample_rate().0 as usize;
    let channels = config.channels() as usize;
    
    // Shared state between callback and main thread
    let resampler = Arc::new(Mutex::new(
        StreamingResampler::new(sample_rate, channels)?
    ));
    let tx = Arc::new(Mutex::new(tx));
    let error_flag = Arc::new(Mutex::new(None::<String>));
    
    let resampler_cb = resampler.clone();
    let tx_cb = tx.clone();
    let error_flag_cb = error_flag.clone();
    
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Err(e) = handle_input_data(data, &resampler_cb, &tx_cb) {
                        *error_flag_cb.lock().unwrap() = Some(e.to_string());
                    }
                },
                move |err| {
                    tracing::error!("Mic input stream error: {}", err);
                },
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let float_data: Vec<f32> = data.iter()
                        .map(|&s| s as f32 / i16::MAX as f32)
                        .collect();
                    if let Err(e) = handle_input_data(&float_data, &resampler_cb, &tx_cb) {
                        *error_flag_cb.lock().unwrap() = Some(e.to_string());
                    }
                },
                move |err| {
                    tracing::error!("Mic input stream error: {}", err);
                },
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            device.build_input_stream(
                &config.into(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let float_data: Vec<f32> = data.iter()
                        .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    if let Err(e) = handle_input_data(&float_data, &resampler_cb, &tx_cb) {
                        *error_flag_cb.lock().unwrap() = Some(e.to_string());
                    }
                },
                move |err| {
                    tracing::error!("Mic input stream error: {}", err);
                },
                None,
            )?
        }
        _ => anyhow::bail!("Unsupported sample format: {:?}", config.sample_format()),
    };
    
    stream.play()?;
    tracing::info!("Microphone capture started");
    
    // Keep stream alive until shutdown
    while !shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
        
        // Check for errors
        if let Some(err) = error_flag.lock().unwrap().take() {
            anyhow::bail!("Mic input error: {}", err);
        }
    }
    
    drop(stream);
    tracing::info!("Microphone capture stopped");
    Ok(())
}

fn handle_input_data(
    data: &[f32],
    resampler: &Arc<Mutex<StreamingResampler>>,
    tx: &Arc<Mutex<mpsc::SyncSender<AudioFrame>>>,
) -> Result<()> {
    // Check if there's actual audio (not just silence)
    let rms = (data.iter().map(|s| s * s).sum::<f32>() / data.len() as f32).sqrt();
    
    let frames = resampler.lock().unwrap().push_samples(data)?;
    
    let tx = tx.lock().unwrap();
    for frame in frames {
        // Log when we send frames (throttled by only logging when there's actual audio)
        if rms > 0.01 {
            tracing::debug!("📡 Mic captured: {} samples, RMS: {:.4}, sending frame to model", data.len(), rms);
        }
        
        if tx.send(frame).is_err() {
            // Receiver dropped, that's ok
            return Ok(());
        }
    }
    
    Ok(())
}
