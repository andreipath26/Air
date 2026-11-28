# Optional direct llama.cpp worker

The default `air-slm` runner remains the local loopback llama.cpp server launched under `prlimit`. It sets CPU-only execution, four threads, mmap loading, Q8_0 K/V cache, prompt cache reuse, bounded context/output, and an RSS watchdog.

The optional `llama-cpp-ffi` feature builds a small C++ worker against a caller-installed llama.cpp C API and launches that worker as a separate child under the same address-space limit. It is offline at runtime and never downloads models. The worker supports local GGUF files for Gemma 4 E2B and SmolLM3 3B, applies the built-in chat template, code-generation sampling defaults (temperature 0.2, top_p 0.95), optional GBNF grammar, Q8_0 KV cache, and trims/reuses the common token prefix with `llama_memory_seq_rm`.

Build requires a compatible local llama.cpp build exposing `llama.h`, `ggml.h`, and `libllama`:

```sh
LLAMA_CPP_INCLUDE_DIR=/path/to/llama.cpp/include \
LLAMA_CPP_GGML_INCLUDE_DIR=/path/to/llama.cpp/ggml/include \
LLAMA_CPP_LIB_DIR=/path/to/llama.cpp/build/bin \
cargo test -p air-slm --features llama-cpp-ffi
```

The opt-in local throughput test uses only a caller-provided GGUF:

```sh
AIR_GGUF_PATH=/models/local.gguf AIR_GGUF_FAMILY=gemma4 \
cargo test -p air-slm --features llama-cpp-ffi local_gguf_throughput_opt_in -- --nocapture
```

Set `AIR_GGUF_FAMILY=smollm3` for SmolLM3; other values select Gemma 4 E2B. With `AIR_GGUF_PATH` unset, the local measurement test exits without loading a model. CI does not enable the native feature or download weights, so headless results validate the mock path/configuration only; they are not real model throughput evidence.
