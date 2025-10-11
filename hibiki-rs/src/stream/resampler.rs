// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::Result;
use rubato::Resampler;

pub const TARGET_SAMPLE_RATE: usize = 24_000;
pub const FRAME_SIZE: usize = 1_920; // 80ms at 24kHz

/// Streaming resampler that converts arbitrary sample rates to 24kHz
/// and buffers frames of exactly 1920 samples (80ms)
pub struct StreamingResampler {
    resampler: rubato::FastFixedIn<f32>,
    input_buffer: Vec<Vec<f32>>,
    output_buffer: Vec<Vec<f32>>,
    input_len: usize,
    accumulated: Vec<f32>,
    channels: usize,
}

impl StreamingResampler {
    pub fn new(input_sample_rate: usize, channels: usize) -> Result<Self> {
        let resample_ratio = TARGET_SAMPLE_RATE as f64 / input_sample_rate as f64;
        let resampler = rubato::FastFixedIn::new(
            resample_ratio,
            f64::max(resample_ratio, 1.0),
            rubato::PolynomialDegree::Septic,
            1024,
            channels,
        )?;
        
        let input_buffer = resampler.input_buffer_allocate(true);
        let output_buffer = resampler.output_buffer_allocate(true);
        
        Ok(Self {
            resampler,
            input_buffer,
            output_buffer,
            input_len: 0,
            accumulated: Vec::new(),
            channels,
        })
    }
    
    /// Push interleaved samples (e.g., [L, R, L, R, ...] for stereo)
    /// Returns vector of complete 1920-sample mono frames
    pub fn push_samples(&mut self, interleaved: &[f32]) -> Result<Vec<[f32; FRAME_SIZE]>> {
        let mut frames = Vec::new();
        
        // Convert interleaved to planar and resample
        let samples_per_channel = interleaved.len() / self.channels;
        let mut pos = 0;
        
        while pos < samples_per_channel {
            let space_in_buffer = self.input_buffer[0].len() - self.input_len;
            let to_copy = usize::min(space_in_buffer, samples_per_channel - pos);
            
            // Copy to input buffer (de-interleave)
            for ch in 0..self.channels {
                for i in 0..to_copy {
                    let idx = (pos + i) * self.channels + ch;
                    self.input_buffer[ch][self.input_len + i] = interleaved[idx];
                }
            }
            
            self.input_len += to_copy;
            pos += to_copy;
            
            // Process when buffer full
            if self.input_len >= self.input_buffer[0].len() {
                let (_, out_len) = self.resampler.process_into_buffer(
                    &self.input_buffer.iter().map(|v| v.as_slice()).collect::<Vec<_>>(),
                    &mut self.output_buffer.iter_mut().map(|v| v.as_mut_slice()).collect::<Vec<_>>(),
                    None,
                )?;
                
                // Downmix to mono: 0.5 * (L + R + ...)
                for i in 0..out_len {
                    let mut sample = 0.0;
                    for ch in 0..self.channels {
                        sample += self.output_buffer[ch][i];
                    }
                    self.accumulated.push(sample / self.channels as f32);
                }
                
                self.input_len = 0;
            }
        }
        
        // Extract complete frames
        while self.accumulated.len() >= FRAME_SIZE {
            let mut frame = [0.0f32; FRAME_SIZE];
            frame.copy_from_slice(&self.accumulated[..FRAME_SIZE]);
            frames.push(frame);
            self.accumulated.drain(..FRAME_SIZE);
        }
        
        Ok(frames)
    }
    
    /// Get any remaining partial frame (used at EOF)
    pub fn flush(&mut self) -> Result<Option<[f32; FRAME_SIZE]>> {
        if self.input_len > 0 {
            // Process remaining samples
            let (_, out_len) = self.resampler.process_partial_into_buffer(
                Some(&self.input_buffer.iter().map(|v| &v[..self.input_len]).collect::<Vec<_>>()),
                &mut self.output_buffer.iter_mut().map(|v| v.as_mut_slice()).collect::<Vec<_>>(),
                None,
            )?;
            
            // Downmix to mono
            for i in 0..out_len {
                let mut sample = 0.0;
                for ch in 0..self.channels {
                    sample += self.output_buffer[ch][i];
                }
                self.accumulated.push(sample / self.channels as f32);
            }
            
            self.input_len = 0;
        }
        
        // Pad to full frame if needed
        if self.accumulated.len() > 0 {
            while self.accumulated.len() < FRAME_SIZE {
                self.accumulated.push(0.0);
            }
            let mut frame = [0.0f32; FRAME_SIZE];
            frame.copy_from_slice(&self.accumulated[..FRAME_SIZE]);
            self.accumulated.clear();
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }
}
