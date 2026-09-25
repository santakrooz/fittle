# Fuzzing

Needs a nightly toolchain and `cargo install cargo-fuzz`.

    cd fuzz
    cargo +nightly fuzz run header -- -max_total_time=300   # header parser + info
    cargo +nightly fuzz run rice   -- -max_total_time=300   # RICE_1 tile decoder
    cargo +nightly fuzz run edit   -- -max_total_time=300   # card formatting round trip

Seed `corpus/header/` with the first blocks of the synthetic corpus to start
from real headers:

    mkdir -p corpus/header
    for f in ../testdata/synthetic/*/*.fit*; do head -c 8640 "$f" > "corpus/header/$(basename "$f" | tr ' ' _)"; done

Any crash lands in `artifacts/<target>/`; add it as a regression test in the
crate before fixing it.
