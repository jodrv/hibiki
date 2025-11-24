#!/bin/bash
# Hibiki-rs Cross-Platform Build Script

set -e  # Exit on error

# Detect platform
OS="$(uname -s)"
FEATURES=""

echo "🔍 Detecting platform..."

case "${OS}" in
    Linux*)
        echo "✓ Detected: Linux"
        
        # Check for NVIDIA GPU
        if command -v nvidia-smi &> /dev/null; then
            echo "✓ NVIDIA GPU detected"
            FEATURES="cuda"
            echo "  Building with CUDA support..."
        else
            echo "ℹ No NVIDIA GPU detected"
            echo "  Building for CPU..."
        fi
        ;;
    Darwin*)
        echo "✓ Detected: macOS"
        FEATURES="metal"
        echo "  Building with Metal support..."
        ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "✓ Detected: Windows"
        
        # Check for NVIDIA GPU
        if command -v nvidia-smi &> /dev/null; then
            echo "✓ NVIDIA GPU detected"
            FEATURES="cuda"
            echo "  Building with CUDA support..."
        else
            echo "ℹ No NVIDIA GPU detected"
            echo "  Building for CPU..."
        fi
        ;;
    *)
        echo "❌ Unknown platform: ${OS}"
        echo "  Building for CPU..."
        ;;
esac

# Build command
if [ -n "${FEATURES}" ]; then
    echo ""
    echo "🔨 Building: cargo build --release --features ${FEATURES}"
    cargo build --release --features "${FEATURES}"
else
    echo ""
    echo "🔨 Building: cargo build --release"
    cargo build --release
fi

echo ""
echo "✅ Build complete!"
echo ""
echo "📦 Binary location: target/release/hibiki"
echo ""
echo "🚀 Quick start:"
if [ -n "${FEATURES}" ]; then
    echo "  List devices: cargo run --features ${FEATURES} -r -- stream --list-devices"
    echo "  Translate:    cargo run --features ${FEATURES} -r -- gen input.mp3 output.wav"
else
    echo "  List devices: cargo run -r -- stream --list-devices"
    echo "  Translate:    cargo run -r -- gen input.mp3 output.wav"
fi
