# Changelog

Notable changes to this crate, newest first. Nothing is recorded here from before the first public release; what this crate changed relative to the fork it came from is described in [ATTRIBUTION.md](ATTRIBUTION.md).

## 0.1.0 — 2026-08-26

First release.

- Opus encoder and decoder (RFC 6716) covering all three coding modes — SILK, CELT and the hybrid of both — at 8, 12, 16, 24 and 48 kHz, mono and stereo, at every one of the nine Opus frame sizes.
- Ogg mux and demux (RFC 7845), so the crate reads and writes `.opus` files rather than only raw packets: `OpusHead`, `OpusTags`, page CRCs and granule positions.
- Multistream encoding and decoding, a repacketizer, packet inspection without decoding, and parallel encoding.
- No C, no FFI, no `build.rs` and no dependencies. Requires Rust 1.88.
