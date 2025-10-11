# hibiki - rust

![rust ci badge](https://github.com/kyutai-labs/moshi/workflows/Rust%20CI/badge.svg)
[![Latest version](https://img.shields.io/crates/v/hibiki.svg)](https://crates.io/crates/hibiki)
[![Documentation](https://docs.rs/hibiki/badge.svg)](https://docs.rs/hibiki)
![License](https://img.shields.io/crates/l/hibiki.svg)

See the [top-level README.md](../README.md) for more information.

This provides the Rust implementation for Hibiki, a real-time speech-to-speech
translation model.

## Requirements

You will need a recent version of the [Rust toolchain](https://rustup.rs/).
To compile GPU support, you will also need the [CUDA](https://developer.nvidia.com/cuda-toolkit) properly installed for your GPU, in particular with `nvcc`.

## Example

```bash
cd hibiki-rs
wget https://github.com/kyutai-labs/moshi/raw/refs/heads/main/data/sample_fr_hibiki_crepes.mp3
cargo run  --features metal -r -- gen sample_fr_hibiki_crepes.mp3 out_en.wav
```
# Hibiki: High-Fidelity Simultaneous Speech-To-Speech Translation

[[Read the paper]][hibiki]
[[Samples]](https://huggingface.co/spaces/kyutai/hibiki-samples)
[[HuggingFace]](https://huggingface.co/collections/kyutai/hibiki-fr-en-67a48835a3d50ee55d37c2b5)
<a target="_blank" href="https://colab.research.google.com/drive/1as2BL2M54ZCYJkSdVYIuRLSW_K305Fye?usp=sharing">
  <img src="https://colab.research.google.com/assets/colab-badge.svg" alt="Open In Colab"/>
</a>

## Description

### What is Hibiki?
Hibiki is a model for **streaming speech translation** (also known as
*simultaneous* translation). Unlike offline translation—where one waits for the end of the source utterance to start
translating--- Hibiki **adapts its flow** to accumulate just enough context to produce a correct translation in real-time,
chunk by chunk. As the user speaks, Hibiki generates natural speech in the target language,
optionally with voice transfer, **along with a text translation**.

### Architecture
Hibiki is a decoder-only model for simultaneous speech translation. Hibiki leverages the **multistream** architecture of
[Moshi](https://arxiv.org/abs/2410.00037) to model source and target speech jointly. This allows Hibiki
to continuously process the input stream while generating the target speech. Hibiki produces text and audio tokens
at a constant framerate of 12.5Hz. This allows for a continuous output audio stream, along with timestamped text translation.

<p align="center">
<img src="./img_hibiki_multistream.png" alt="Schema representing the multistream architecture of Hibiki"
width="650px"></p>

### How is it trained?

Hibiki relies on supervised training of aligned source speech and target speech and text, from the same speaker.
Such data does not exist in significant amounts so we rely on synthetic data generation. Word-level matching is made
between source and target transcripts using a *contextual alignment* weakly-supervised method that leverages an
off-the-shelf [MADLAD](https://huggingface.co/google/madlad400-3b-mt) machine translation system. The derived alignment
rule (a word should only appear in the target once it's predictable from the source) is applied either by inserting
silences or by synthesizing targets with a voice controlled, alignment-aware TTS.

<div style="display: flex; align-items: center; justify-content: center;">
  <img src="./img_contextual_alignment_text.png"
       alt="Text-based alignemnt of source and target sequences."
       style="margin-right: 20px;"/
       width="400px">
  <img src="./img_synthetic_waveforms.png"
       alt="Generating synthetic data with silence insertion and alignment-aware TTS"
       width="400px">
</div>

### Inference
At inference, Hibiki continuously encodes source speech and produces target speech. Hibiki relies on simple
temperature sampling and is thus compatible with batching unlike models that rely on complex
inference policies. Moreover, the fidelity of Hibiki's voice transfer can be controlled by changing the coefficient of
the Classifier-Free Guidance: a larger coefficient will increase voice similarity, but excessive coefficients can lead
to worse translations. Hibiki currently only supports French-to-English translation. Its smaller alternative, Hibiki-M
can run locally on smartphone hardware. Current models were trained on sequences up to 120 seconds and use a context
size of 40 seconds.

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
cargo run  --features metal -r -- gen sample_fr_hibiki_crepes.mp3 out_en.wav
```

#### Real-time Streaming (`stream` command)

The `stream` command enables real-time speech-to-speech translation with live audio input (microphone or file) and output (speakers and/or WAV file).

**List available audio devices:**
```bash
cargo run --features metal -r -- stream --list-devices
```

**File → Speaker (default):**
```bash
# macOS Metal
CXXFLAGS="-I$(xcrun --show-sdk-path)/usr/include/c++/v1" \
cargo run --features metal -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3
```

**File → Speaker + Save to WAV:**
```bash
# macOS Metal
CXXFLAGS="-I$(xcrun --show-sdk-path)/usr/include/c++/v1" \
cargo run --features metal -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3 \
  --save-output out.wav
```

**File → WAV only (no speaker):**
```bash
# macOS Metal
CXXFLAGS="-I$(xcrun --show-sdk-path)/usr/include/c++/v1" \
cargo run --features metal -r -- stream \
  --input-file sample_fr_hibiki_crepes.mp3 \
  --disable-speaker \
  --save-output test_file_to_file_only.wav
```

**Microphone → Speaker:**
```bash
# macOS Metal
CXXFLAGS="-I$(xcrun --show-sdk-path)/usr/include/c++/v1" \
cargo run --features metal -r -- stream \
  --input-device "MacBook"
```

**Microphone → Specific output device:**
```bash
# macOS Metal
CXXFLAGS="-I$(xcrun --show-sdk-path)/usr/include/c++/v1" \
cargo run --features metal -r -- stream \
  --input-device "MacBook" \
  --output-device "realme"
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

You can use either `--features cuda` to run on a nvidia gpu, or
`--features metal` to run on a mac.

## Models

We release two models for `FR -> EN` translation:
- Hibiki 2B (for the backbone, a bit more with the depth transformer), 16 RVQ per stream.
- Hibiki 1B (for the backbone, a bit more with the depth transformer), 8 RVQ per stream, ideal for on device inferencde.

Depending on the backend, the file format will vary.  Quantized models coming up soon. Current list of models:
- Hibiki 2B for PyTorch (bf16): [kyutai/hibiki-2b-pytorch-bf16](https://huggingface.co/kyutai/hibiki-2b-pytorch-bf16)
- Hibiki 1B for PyTorch (bf16): [kyutai/hibiki-1b-pytorch-bf16](https://huggingface.co/kyutai/hibiki-1b-pytorch-bf16)
- Hibiki 2B for MLX (bf16): [kyutai/hibiki-2b-mlx-bf16](https://huggingface.co/kyutai/hibiki-2b-mlx-bf16)
- Hibiki 1B for MLX (bf16): [kyutai/hibiki-1b-mlx-bf16](https://huggingface.co/kyutai/hibiki-1b-mlx-bf16)

All models are released under the CC-BY 4.0 license.


## License

The present code is provided under the MIT license for the Python parts, and Apache license for the Rust backend.
The web client code is provided under the MIT license.

The weights for the models are released under the CC-BY 4.0 license.

## Citation

If you use Hibiki, please cite the following paper,

```
@misc{kyutai2025hibiki,
      title={High-Fidelity Simultaneous Speech-To-Speech Translation},
      author={Tom Labiausse and Laurent Mazar\'e and Edouard Grave and
      Patrick P\'erez and Alexandre D\'efossez and Neil Zeghidour},
      year={2025},
      eprint={2502.03382},
      archivePrefix={arXiv},
      primaryClass={cs.CL},
      url={https://arxiv.org/abs/2502.03382},
}
```



[hibiki]: https://arxiv.org/abs/2502.03382