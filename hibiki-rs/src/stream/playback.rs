// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::resampler::TARGET_SAMPLE_RATE;

const RING_BUFFER_SIZE: usize = TARGET_SAMPLE_RATE * 12; // 12 seconds - needed for slower 2B model
const PAUSE_THRESHOLD: usize = TARGET_SAMPLE_RATE / 100; // 0.1s = 2400 samples - pause when buffer critically low
const RESUME_THRESHOLD: usize = TARGET_SAMPLE_RATE / 10; // 0.25s = 6000 samples - resume when buffer refilled (MUST BE > PAUSE!)
const INITIAL_FILL_THRESHOLD: usize = TARGET_SAMPLE_RATE / 10; // 0.5s = 12000 samples - wait longer for 2B to generate

struct PlaybackBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    read_pos: usize,
}

impl PlaybackBuffer {
    fn new() -> Self {
        Self {
            buffer: vec![0.0; RING_BUFFER_SIZE],
            write_pos: 0,
            read_pos: 0,
        }
    }
    
    fn available(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            RING_BUFFER_SIZE - self.read_pos + self.write_pos
        }
    }
    
    fn read(&mut self, count: usize, out: &mut Vec<f32>) {
        out.clear();
        let available = self.available();
        let to_read = count.min(available);
        
        if to_read == 0 {
            return;
        }
        
        // Handle wrap-around
        if self.read_pos + to_read <= RING_BUFFER_SIZE {
            out.extend_from_slice(&self.buffer[self.read_pos..self.read_pos + to_read]);
            self.read_pos = (self.read_pos + to_read) % RING_BUFFER_SIZE;
        } else {
            let first_chunk = RING_BUFFER_SIZE - self.read_pos;
            out.extend_from_slice(&self.buffer[self.read_pos..]);
            out.extend_from_slice(&self.buffer[..to_read - first_chunk]);
            self.read_pos = to_read - first_chunk;
        }
    }
    
    fn write(&mut self, samples: &[f32]) -> bool {
        let available = self.available();
        let free = RING_BUFFER_SIZE - available - 1; // -1 to distinguish full from empty
        let overflowed = samples.len() > free;
        
        if overflowed {
            // Drop oldest samples by advancing read pointer
            let to_drop = samples.len() - free;
            self.read_pos = (self.read_pos + to_drop) % RING_BUFFER_SIZE;
        }
        
        let to_write = samples.len().min(free);
        
        // Handle wrap-around
        if self.write_pos + to_write <= RING_BUFFER_SIZE {
            self.buffer[self.write_pos..self.write_pos + to_write].copy_from_slice(&samples[..to_write]);
            self.write_pos = (self.write_pos + to_write) % RING_BUFFER_SIZE;
        } else {
            let first_chunk = RING_BUFFER_SIZE - self.write_pos;
            self.buffer[self.write_pos..].copy_from_slice(&samples[..first_chunk]);
            self.buffer[..to_write - first_chunk].copy_from_slice(&samples[first_chunk..to_write]);
            self.write_pos = to_write - first_chunk;
        }
        
        overflowed
    }
}

pub struct SpeakerSink {
    buffer: Arc<Mutex<PlaybackBuffer>>,
    _stream: cpal::Stream,
    playing: Arc<AtomicBool>,
    started: Arc<AtomicBool>, // Track if we've ever started (for initial fill)
    underrun_count: Arc<AtomicU64>,
    overflow_count: Arc<AtomicU64>,
}

impl SpeakerSink {
    pub fn new(device: cpal::Device) -> Result<Self> {
        // CRITICAL: Force 24kHz output to avoid resampling artifacts!
        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(TARGET_SAMPLE_RATE as u32),
            buffer_size: cpal::BufferSize::Default,
        };
        
        tracing::info!(
            "Speaker output config: {} channels, {} Hz (forced, no resampling)",
            config.channels,
            config.sample_rate.0,
        );
        
        let sample_rate = config.sample_rate.0 as usize;
        let channels = config.channels as usize;
        
        // Optimized playback buffer with read cursor
        let buffer = Arc::new(Mutex::new(PlaybackBuffer::new()));
        let buffer_cb = buffer.clone();
        
        let playing = Arc::new(AtomicBool::new(false)); // Start paused until buffer fills
        let playing_cb = playing.clone();
        
        let started = Arc::new(AtomicBool::new(false)); // Track initial fill
        let started_cb = started.clone();
        
        let underrun_count = Arc::new(AtomicU64::new(0));
        let underrun_count_cb = underrun_count.clone();
        
        let overflow_count = Arc::new(AtomicU64::new(0));
        
        // No resampler needed - we force 24kHz output!
        
        let stream = device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        thread_local! {
                            static TEMP_BUF: std::cell::RefCell<Vec<f32>> = std::cell::RefCell::new(Vec::with_capacity(4096));
                            static LAST_LOG: std::cell::Cell<std::time::Instant> = std::cell::Cell::new(std::time::Instant::now());
                        }
                        
                        let frames = data.len() / channels;
                        
                        // Check buffer level first WITHOUT draining
                        let buffer_len = buffer_cb.lock().unwrap().available();
                        let is_playing = playing_cb.load(Ordering::Relaxed);
                        let has_started = started_cb.load(Ordering::Relaxed);
                        
                        // Smarter hysteresis with initial fill requirement
                        if !has_started {
                            if buffer_len >= INITIAL_FILL_THRESHOLD {
                                started_cb.store(true, Ordering::Relaxed);
                                playing_cb.store(true, Ordering::Relaxed);
                                tracing::info!("🎵 Playback STARTED: initial buffer = {} samples ({:.2}s)", 
                                    buffer_len, buffer_len as f32 / TARGET_SAMPLE_RATE as f32);
                            } else {
                                data.fill(0.0);
                                LAST_LOG.with(|last| {
                                    if last.get().elapsed().as_millis() > 500 {
                                        tracing::info!("⏳ Buffering... {}/{} samples ({:.1}%)", 
                                            buffer_len, INITIAL_FILL_THRESHOLD,
                                            100.0 * buffer_len as f32 / INITIAL_FILL_THRESHOLD as f32);
                                        last.set(std::time::Instant::now());
                                    }
                                });
                                return;  // Don't drain buffer yet!
                            }
                        }
                        
                        // After initial start: use tighter hysteresis
                        if !is_playing && buffer_len >= RESUME_THRESHOLD {
                            playing_cb.store(true, Ordering::Relaxed);
                            tracing::warn!("▶️  RESUMED: buffer refilled to {} samples ({:.2}s)", 
                                buffer_len, buffer_len as f32 / TARGET_SAMPLE_RATE as f32);
                        } else if is_playing && buffer_len < PAUSE_THRESHOLD {
                            playing_cb.store(false, Ordering::Relaxed);
                            tracing::error!("⏸️  PAUSED: buffer depleted to {} samples ({:.2}s) - UNDERRUN!", 
                                buffer_len, buffer_len as f32 / TARGET_SAMPLE_RATE as f32);
                            underrun_count_cb.fetch_add(1, Ordering::Relaxed);
                        }
                        
                        // NOW read from buffer (only if playing)
                        let to_read = if playing_cb.load(Ordering::Relaxed) {
                            let mut buf = buffer_cb.lock().unwrap();
                            TEMP_BUF.with(|temp| {
                                let mut temp = temp.borrow_mut();
                                buf.read(frames, &mut temp);
                                temp.len()
                            })
                        } else {
                            0
                        };
                        
                        // Write samples WITHOUT holding any lock
                        if to_read > 0 {
                            TEMP_BUF.with(|temp| {
                                let temp = temp.borrow();
                                
                                for i in 0..to_read {
                                    let sample = temp[i];
                                    for ch in 0..channels {
                                        data[i * channels + ch] = sample;
                                    }
                                }
                                // Fill remainder with silence if needed
                                for i in (to_read * channels)..(data.len()) {
                                    data[i] = 0.0;
                                }
                            });
                        } else {
                            data.fill(0.0);
                        }
                    },
                    move |err| {
                        tracing::error!("Speaker output stream error: {}", err);
                    },
                    None,
                )?;
        
        stream.play()?;
        tracing::info!("Speaker playback started");
        
        Ok(Self {
            buffer,
            _stream: stream,
            playing,
            started,
            underrun_count,
            overflow_count,
        })
    }
    
    /// Push samples to playback (non-blocking)
    pub fn push_samples(&mut self, samples: &[f32]) -> Result<()> {
        // No resampling needed - direct write at 24kHz
        let mut buf = self.buffer.lock().unwrap();
        let before = buf.available();
        if buf.write(samples) {
            self.overflow_count.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("🚨 Buffer OVERFLOW! Dropped samples. Buffer was at {} samples", before);
        }
        let after = buf.available();
        tracing::debug!("📥 Pushed {} samples to buffer (level: {} → {})", samples.len(), before, after);
        Ok(())
    }
    
    pub fn buffer_level(&self) -> usize {
        self.buffer.lock().unwrap().available()
    }
    
    pub fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::SeqCst)
    }
    
    pub fn overflow_count(&self) -> u64 {
        self.overflow_count.load(Ordering::Relaxed)
    }
}
