

## Running the model

We provide inference code for PyTorch, Rust, MLX for macOS, and MLX-swift
for iOS. Note that the implementation for Hibiki is very close to that of Moshi, and the actual code
is in the [kyutai-labs/moshi](https://github.com/kyutai-labs/moshi) repository.

### PyTorch

In order to translate an audio file using Hibiki/PyTorch, install the
`moshi` package via.
```bash
pip install -U moshi
```

Then you can retrieve some sample files from [kyutai-labs/moshi](https://github.com/kyutai-labs/moshi/tree/main/data)
and translate them via the following:
```bash
wget https://github.com/kyutai-labs/moshi/raw/refs/heads/main/data/sample_fr_hibiki_crepes.mp3
python -m moshi.run_inference sample_fr_hibiki_crepes.mp3 out_en.wav --hf-repo kyutai/hibiki-1b-pytorch-bf16
```


You can specify some classifier-free guidance using the `--cfg-coef` parameter.
The default value is 1, the higher the value, the closer the generated voice
should be to the original voice. A typical value to use is 3.

### MLX

In order to translate an audio file using Hibiki/MLX, install the
`moshi_mlx` package via the following command. You need at least version `0.2.1`
of this package.

```bash
pip install -U moshi_mlx
```

Then you can retrieve some sample files from [kyutai-labs/moshi](https://github.com/kyutai-labs/moshi/tree/main/data)
and translate them via the following:
```bash
wget https://github.com/kyutai-labs/moshi/raw/refs/heads/main/data/sample_fr_hibiki_crepes.mp3
python -m moshi_mlx.run_inference sample_fr_hibiki_crepes.mp3 out_en.wav --hf-repo kyutai/hibiki-1b-mlx-bf16
```

You can specify some classifier-free guidance using the `--cfg-coef` parameter.
The default value is 1, the higher the value, the closer the generated voice
should be to the original voice. A typical value to use is 3.

You can also use the model in real-time via the web-ui by running the following
command.
```bash
python -m moshi_mlx.local_web --hf-repo kyutai/hibiki-1b-mlx-bf16
```

### MLX-Swift

The [kyutai-labs/moshi-swift](https://github.com/kyutai-labs/moshi-swift) repo
contains a MLX-Swift implementation that can run on an iPhone. This was tested
on an iPhone 16 Pro. Note that this code there is very much experimental.

### Rust

The [hibiki-rs](https://github.com/kyutai-labs/hibiki/tree/main/hibiki-rs)
directory contains a Rust implementation with two modes:

#### Offline Translation (`gen` command)

Translate an entire audio file at once:

```bash
cd hibiki-rs
wget https://github.com/kyutai-labs/moshi/raw/refs/heads/main/data/sample_fr_hibiki_crepes.mp3

# On macOS (Metal)
cargo run --features metal -r -- gen sample_fr_hibiki_crepes.mp3 out_en.wav

# On Linux with NVIDIA GPU (CUDA)
cargo run --features cuda -r -- gen sample_fr_hibiki_crepes.mp3 out_en.wav

# On CPU (any platform)
cargo run -r -- gen sample_fr_hibiki_crepes.mp3 out_en.wav
```

#### Real-time Streaming (`stream` command)

The `stream` command enables real-time speech-to-speech translation with live audio input (microphone or file) and output (speakers and/or WAV file).

**List available audio devices:**

*macOS:*
```bash
cargo run --features metal -r -- stream --list-devices
```

*Linux with CUDA:*
```bash
cargo run --features cuda -r -- stream --list-devices
```

*CPU:*
```bash
cargo run -r -- stream --list-devices
```

**File → Speaker (default):**

*macOS:*
```bash
cargo run --features metal -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3
```

*Linux with CUDA:*
```bash
cargo run --features cuda -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3
```

**File → Speaker + Save to WAV:**

*macOS:*
```bash
cargo run --features metal -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3 \
  --save-output out.wav
```

*Linux with CUDA:*
```bash
cargo run --features cuda -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3 \
  --save-output out.wav
```

**Microphone → Speaker:**

*macOS:*
```bash
cargo run --features metal -r -- stream \
  --input-device "MacBook"
```

*Linux with CUDA:*
```bash
cargo run --features cuda -r -- stream \
  --input-device "pulse"
```

**Stream command options:**
- `--input-file <path>`: Input audio file (mp3/wav/flac)
- `--input-device "<name>"`: Input device (substring match, case-insensitive)
- `--output-device "<name>"`: Output device (substring match, case-insensitive)
- `--disable-speaker`: Disable speaker output
- `--save-output <path.wav>`: Save generated audio to WAV file (24kHz, 16-bit PCM, mono)
- `--list-devices`: List available audio devices and exit

The streaming mode automatically:
- Resamples any input rate to 24 kHz
- Converts stereo to mono
- Paces file playback to real-time
- Handles device sample rate mismatches
- Applies TPDF dither when saving to 16-bit WAV

**Platform-Specific Features:**
- Use `--features metal` on macOS to enable Metal GPU acceleration
- Use `--features cuda` on Linux/Windows to enable NVIDIA CUDA GPU acceleration  
- Omit feature flags to run on CPU (works on all platforms)
- **Note:** The code automatically detects and uses available accelerators at runtime
