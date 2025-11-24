# Hibiki-rs Troubleshooting Guide

## Cross-Platform Compatibility Issues

### ❌ Problem: Build fails with `objc/NSObject.h: No such file or directory`

**Root Cause:** Using `--features metal` on Linux/Windows. Metal is Apple-specific.

**Solution:**
```bash
# ❌ WRONG (on Linux)
cargo run --features metal -r -- stream --list-devices

# ✅ CORRECT (on Linux with NVIDIA GPU)
cargo run --features cuda -r -- stream --list-devices

# ✅ CORRECT (on Linux without GPU)
cargo run -r -- stream --list-devices
```

---

### ❌ Problem: `openssl` required by crate `openssl-sys` was not found

**Root Cause:** Missing OpenSSL development libraries.

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install -y libssl-dev pkg-config
```

**Solution (macOS):**
OpenSSL should be included with Xcode Command Line Tools. If not:
```bash
brew install openssl
```

**Solution (Windows):**
Follow the [rust-openssl Windows guide](https://github.com/sfackler/rust-openssl#windows)

---

### ❌ Problem: `alsa` required by crate `alsa-sys` was not found

**Root Cause:** Missing ALSA audio development libraries (Linux only).

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get install -y libasound2-dev
```

**Solution (Fedora/RHEL):**
```bash
sudo dnf install alsa-lib-devel
```

---

### ❌ Problem: `cmake` not installed

**Root Cause:** CMake build system not installed (needed for sentencepiece).

**Solution (Ubuntu/Debian):**
```bash
sudo apt-get install -y cmake
```

**Solution (macOS):**
```bash
brew install cmake
```

**Solution (Windows):**
Download from https://cmake.org/download/

---

### ❌ Problem: CUDA not found or compilation fails

**Root Cause:** CUDA toolkit not properly installed or `nvcc` not in PATH.

**Solution:**
1. Install NVIDIA CUDA Toolkit: https://developer.nvidia.com/cuda-toolkit
2. Verify installation:
   ```bash
   nvidia-smi
   nvcc --version
   ```
3. Add CUDA to PATH:
   ```bash
   export PATH=/usr/local/cuda/bin:$PATH
   export LD_LIBRARY_PATH=/usr/local/cuda/lib64:$LD_LIBRARY_PATH
   ```
4. Rebuild:
   ```bash
   cargo clean
   cargo build --release --features cuda
   ```

---

## Quick Platform Reference

| Platform | Command | When to Use |
|----------|---------|-------------|
| **macOS** | `cargo run --features metal -r` | Always (Metal is native) |
| **Linux + NVIDIA GPU** | `cargo run --features cuda -r` | You have NVIDIA GPU |
| **Linux (no GPU)** | `cargo run -r` | No NVIDIA GPU or want CPU |
| **Windows + NVIDIA GPU** | `cargo run --features cuda -r` | You have NVIDIA GPU |
| **Windows (no GPU)** | `cargo run -r` | No NVIDIA GPU or want CPU |

---

## Common Workflow Issues

### Issue: Code worked on macOS but fails on Ubuntu

**What happened:**
- macOS: Built with `--features metal` ✅
- Ubuntu: Tried `--features metal` ❌ (Metal doesn't exist on Linux)

**Solution:**
Build without the metal feature on Linux:
```bash
# On Ubuntu with NVIDIA GPU
cargo build --release --features cuda

# On Ubuntu without GPU
cargo build --release
```

---

### Issue: How do I know which feature to use?

**Detection script:**
```bash
# Use the provided build.sh script (auto-detects platform)
./build.sh

# Or manually check:
# 1. Check platform
uname -s

# 2. Check for NVIDIA GPU
nvidia-smi

# 3. Choose feature:
# - macOS → use metal
# - Linux + nvidia-smi works → use cuda
# - Linux + no nvidia-smi → use no features (CPU)
```

---

## Performance Issues

### Issue: Model runs slowly

**Possible causes:**
1. Running on CPU instead of GPU
2. Wrong feature flag for your hardware
3. GPU has insufficient memory

**Solutions:**

**Verify GPU is being used:**
```bash
# While running, check GPU usage
nvidia-smi  # For NVIDIA GPUs

# macOS: Activity Monitor > GPU History
```

**Check which device is selected:**
The code auto-detects in this order:
1. CUDA (if available and built with `cuda` feature)
2. Metal (if available and built with `metal` feature)
3. CPU (fallback)

**Force CPU to compare:**
```bash
cargo run --features cuda -r -- stream --cpu --list-devices
```

---

## Audio Device Issues

### Issue: No audio devices listed

**Solution:**
```bash
# List devices with correct feature for your platform
# macOS:
cargo run --features metal -r -- stream --list-devices

# Linux:
cargo run --features cuda -r -- stream --list-devices
# or
cargo run -r -- stream --list-devices
```

---

### Issue: "Device not found" error

**Cause:** Device name substring doesn't match any available device.

**Solution:**
1. List all devices first
2. Use a substring that uniquely identifies your device (case-insensitive)
3. Examples:
   - `--input-device "usb"` → matches "USB Audio Device"
   - `--output-device "realme"` → matches "realme Buds Air 3"

---

## Build Performance

### Issue: Build takes too long

**Tips:**
```bash
# 1. Use incremental builds (default, but verify)
export CARGO_INCREMENTAL=1

# 2. Build in release mode only when needed
cargo build  # Debug mode (faster build)
cargo run -- stream --list-devices  # Test in debug

# 3. Use all CPU cores
cargo build -j $(nproc)

# 4. Cache dependencies
# First build caches deps, subsequent builds are faster
```

---

## Still Having Issues?

1. **Check your platform:**
   ```bash
   uname -a
   rustc --version
   cargo --version
   ```

2. **Clean and rebuild:**
   ```bash
   cargo clean
   cargo build --release --features <correct-feature>
   ```

3. **Verify all dependencies:**
   ```bash
   # Ubuntu/Debian
   dpkg -l | grep -E "libssl-dev|libasound2-dev|pkg-config"
   
   # macOS
   xcode-select -p
   ```

4. **Check GitHub Issues:**
   https://github.com/kyutai-labs/hibiki/issues
