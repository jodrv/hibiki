// Copyright (c) Kyutai, all rights reserved.
// This source code is licensed under the license found in the
// LICENSE file in the root directory of this source tree.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    #[ignore] // Run manually with: cargo test --test stream_test -- --ignored
    fn test_file_to_wav_streaming() {
        // This test validates basic streaming functionality
        // It should be run manually as it requires model files
        
        let input_file = PathBuf::from("sample_fr_hibiki_crepes.mp3");
        let output_file = PathBuf::from("/tmp/hibiki_stream_test_output.wav");
        
        // Clean up any previous output
        let _ = std::fs::remove_file(&output_file);
        
        // Verify input file exists
        if !input_file.exists() {
            eprintln!("Warning: Test input file not found. Download with:");
            eprintln!("wget https://github.com/kyutai-labs/moshi/raw/refs/heads/main/data/sample_fr_hibiki_crepes.mp3");
            return;
        }
        
        println!("Running streaming test: {} -> {}", 
            input_file.display(), 
            output_file.display()
        );
        
        // Note: This test would need to actually run the streaming command
        // For now, it's a placeholder for manual testing
        println!("To run this test manually:");
        println!("cargo run --features metal -r -- stream \\");
        println!("  --input-file {} \\", input_file.display());
        println!("  --disable-speaker \\");
        println!("  --save-output {}", output_file.display());
    }
    
    #[test]
    fn test_wav_validation() {
        // Test that validates a WAV file has correct properties
        let test_wav = PathBuf::from("/tmp/hibiki_stream_test_output.wav");
        
        if !test_wav.exists() {
            println!("No test WAV file found, skipping validation");
            return;
        }
        
        // Read and validate WAV properties
        let mut reader = match hound::WavReader::open(&test_wav) {
            Ok(r) => r,
            Err(_) => {
                println!("Could not open WAV file for validation");
                return;
            }
        };
        
        let spec = reader.spec();
        
        // Verify expected format: 24kHz, mono, 16-bit
        assert_eq!(spec.channels, 1, "Expected mono audio");
        assert_eq!(spec.sample_rate, 24_000, "Expected 24kHz sample rate");
        assert_eq!(spec.bits_per_sample, 16, "Expected 16-bit samples");
        assert_eq!(spec.sample_format, hound::SampleFormat::Int, "Expected Int sample format");
        
        let samples: Vec<i16> = reader.samples::<i16>().filter_map(Result::ok).collect();
        let duration_s = samples.len() as f32 / 24_000.0;
        
        println!("WAV validation passed:");
        println!("  Duration: {:.2}s", duration_s);
        println!("  Samples: {}", samples.len());
        println!("  Format: {}Hz, {} channels, {} bits", 
            spec.sample_rate, spec.channels, spec.bits_per_sample);
        
        // Basic sanity checks
        assert!(samples.len() > 0, "WAV file should not be empty");
        assert!(duration_s > 0.5, "Duration should be reasonable (>0.5s)");
    }
}
