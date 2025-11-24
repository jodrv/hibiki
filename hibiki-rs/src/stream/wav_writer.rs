// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

use anyhow::Result;
use std::path::Path;
use std::sync::mpsc;

use super::resampler::TARGET_SAMPLE_RATE;

/// Simple TPDF dither for f32 -> i16 conversion
fn dither_f32_to_i16(sample: f32, rng: &mut u32) -> i16 {
    // TPDF: sum of two uniform random numbers
    let r1 = (*rng as f32 / u32::MAX as f32) - 0.5;
    *rng = rng.wrapping_mul(1103515245).wrapping_add(12345); // Simple LCG
    let r2 = (*rng as f32 / u32::MAX as f32) - 0.5;
    *rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
    
    let dither = (r1 + r2) / 32768.0; // Scale for 16-bit
    let dithered = sample + dither;
    (dithered.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// Runs WAV writer thread
pub fn run_wav_writer<P: AsRef<Path>>(
    path: P,
    rx: mpsc::Receiver<Vec<f32>>,
) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    
    let mut writer = hound::WavWriter::create(path.as_ref(), spec)?;
    let mut rng = 0x12345678u32; // Seed for dither
    let mut total_samples = 0;
    
    tracing::info!("WAV writer started: {:?}", path.as_ref());
    
    while let Ok(samples) = rx.recv() {
        for &sample in &samples {
            let sample_i16 = dither_f32_to_i16(sample, &mut rng);
            writer.write_sample(sample_i16)?;
            total_samples += 1;
        }
    }
    
    writer.finalize()?;
    let duration_s = total_samples as f32 / TARGET_SAMPLE_RATE as f32;
    tracing::info!(
        "WAV file saved: {:?} ({} samples, {:.2}s)",
        path.as_ref(),
        total_samples,
        duration_s
    );
    
    Ok(())
}
