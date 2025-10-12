# Building hibiki-rs - Platform-Specific Guide

This guide provides platform-specific build instructions for hibiki-rs.

## Ubuntu/Linux

### Prerequisites
```bash
# Install required development libraries
sudo apt-get update
sudo apt-get install -y libssl-dev libasound2-dev pkg-config cmake
```

**What these are for:**
- `libssl-dev`: OpenSSL for secure connections (HuggingFace model downloads)
- `libasound2-dev`: ALSA audio system for microphone/speaker support
- `pkg-config`: Build tool for finding installed libraries
- `cmake`: Build system for C++ dependencies (sentencepiece)

### Option 1: With CUDA (Recommended for NVIDIA GPU)
```bash
# Verify CUDA is available
nvidia-smi

# Build with CUDA support
cargo build --release --features cuda

# Run examples
cargo run --features cuda -r -- stream --list-devices
cargo run --features cuda -r -- gen input.mp3 output.wav
```

### Option 2: CPU Only (No GPU)
```bash
# Build without features
cargo build --release

# Run examples
cargo run -r -- stream --list-devices
cargo run -r -- gen input.mp3 output.wav
```

## macOS

### Prerequisites
```bash
# Install Xcode Command Line Tools (optional but recommended)
xcode-select --install
```

### With Metal (Default for macOS)
```bash
# Build with Metal support
cargo build --release --features metal

# Run examples
cargo run --features metal -r -- stream --list-devices
cargo run --features metal -r -- gen input.mp3 output.wav
```

## Important Notes

### ⚠️ Don't Use Metal Feature on Linux
**Never use `--features metal` on Linux!** This will cause compilation errors because Metal is Apple-specific.

### ⚠️ Don't Use CUDA Feature on macOS without NVIDIA GPU
macOS typically doesn't have NVIDIA GPUs. Use `metal` instead.

### Runtime Auto-Detection
The code automatically detects available accelerators at runtime:
1. Checks for CUDA (if built with cuda feature)
2. Checks for Metal (if built with metal feature)  
3. Falls back to CPU

### Quick Reference
| Platform | GPU | Build Command |
|----------|-----|---------------|
| Linux | NVIDIA | `cargo build --release --features cuda` |
| Linux | None | `cargo build --release` |
| macOS | Apple Silicon | `cargo build --release --features metal` |
| macOS | Intel | `cargo build --release --features metal` |
| Windows | NVIDIA | `cargo build --release --features cuda` |
| Windows | None | `cargo build --release` |

## Common Errors

### Error: `objc/NSObject.h: No such file or directory`
**Cause:** You're trying to use `--features metal` on Linux.  
**Solution:** Remove the `metal` feature and use `cuda` or no features.

### Error: `openssl` required by crate `openssl-sys` was not found
**Cause:** Missing OpenSSL development libraries.  
**Solution:** Install `libssl-dev`:
```bash
sudo apt-get install -y libssl-dev pkg-config
```

### Error: `alsa` required by crate `alsa-sys` was not found
**Cause:** Missing ALSA (audio system) development libraries on Linux.  
**Solution:** Install `libasound2-dev`:
```bash
sudo apt-get install -y libasound2-dev
```

### Error: CUDA not found
**Cause:** CUDA toolkit not properly installed.  
**Solution:** Install NVIDIA CUDA Toolkit from https://developer.nvidia.com/cuda-toolkit
