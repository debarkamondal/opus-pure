//! Port of libopus `src/repacketizer.c` + the packet helpers from `src/opus.c`:
//! split Opus packets into frames and recombine/re-frame/pad them WITHOUT
//! re-encoding. Used to merge several packets into a longer one, split a
//! multi-frame packet, or pad a packet to a target size (e.g. for CBR
//! transport). All frames in a repacketizer must share the same TOC config
//! (mode/bandwidth/frame-size); only the code (0..3) and framing change.

use crate::packet::frame_count;
use crate::{Error, Result};

/// opus_packet_get_samples_per_frame(toc, Fs).
pub(crate) fn samples_per_frame(toc: u8, fs: i32) -> i32 {
    if toc & 0x80 != 0 {
        let a = ((toc >> 3) & 0x3) as i32;
        (fs << a) / 400
    } else if toc & 0x60 == 0x60 {
        if toc & 0x08 != 0 { fs / 50 } else { fs / 100 }
    } else {
        let a = ((toc >> 3) & 0x3) as i32;
        if a == 3 {
            fs * 60 / 1000
        } else {
            (fs << a) / 100
        }
    }
}

/// Decode one frame-length field (libopus `parse_size`).
///
/// Returns `(bytes_consumed, size)`; a negative `size` means the field is
/// malformed.
fn parse_size(data: &[u8]) -> (i32, i32) {
    if data.is_empty() {
        (-1, -1)
    } else if data[0] < 252 {
        (1, data[0] as i32)
    } else if data.len() < 2 {
        (-1, -1)
    } else {
        (2, data[1] as i32 * 4 + data[0] as i32)
    }
}

fn encode_size(size: i32, out: &mut Vec<u8>) {
    if size < 252 {
        out.push(size as u8);
    } else {
        let b0 = 252 + (size & 0x3);
        out.push(b0 as u8);
        out.push(((size - b0) >> 2) as u8);
    }
}

/// Split `data` into its frames. Returns (toc, frame byte-ranges, packet_offset).
/// `self_delimited` parses the trailing length prefix used by multistream.
#[allow(clippy::type_complexity)]
pub(crate) fn parse_packet(
    data: &[u8],
    self_delimited: bool,
) -> Result<(u8, Vec<(usize, usize)>, usize)> {
    if data.is_empty() {
        return Err(Error::InvalidPacket("invalid packet"));
    }
    let framesize = samples_per_frame(data[0], 48000);
    let toc = data[0];
    let mut pos = 1usize; // cursor into data
    let mut len = data.len() as i32 - 1;
    let mut cbr = false;
    let mut last_size = len;
    let mut sizes: Vec<i32> = Vec::new();

    let count: usize = match toc & 0x3 {
        0 => 1,
        1 => {
            cbr = true;
            if !self_delimited {
                if len & 1 != 0 {
                    return Err(Error::InvalidPacket("invalid packet"));
                }
                last_size = len / 2;
                sizes.push(last_size);
            }
            2
        }
        2 => {
            let (bytes, sz) = parse_size(&data[pos..]);
            if bytes < 0 {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            len -= bytes;
            if sz < 0 || sz > len {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            pos += bytes as usize;
            sizes.push(sz);
            last_size = len - sz;
            2
        }
        _ => {
            if len < 1 {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            let ch = data[pos];
            pos += 1;
            len -= 1;
            let count = (ch & 0x3f) as usize;
            if count == 0 || framesize * count as i32 > 5760 {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            if ch & 0x40 != 0 {
                // padding
                loop {
                    if len <= 0 {
                        return Err(Error::InvalidPacket("invalid packet"));
                    }
                    let p = data[pos];
                    pos += 1;
                    len -= 1;
                    let tmp = if p == 255 { 254 } else { p as i32 };
                    len -= tmp;
                    if p != 255 {
                        break;
                    }
                }
            }
            if len < 0 {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            cbr = ch & 0x80 == 0;
            if !cbr {
                last_size = len;
                for _ in 0..count - 1 {
                    let (bytes, sz) = parse_size(&data[pos..]);
                    if bytes < 0 {
                        return Err(Error::InvalidPacket("invalid packet"));
                    }
                    len -= bytes;
                    if sz < 0 || sz > len {
                        return Err(Error::InvalidPacket("invalid packet"));
                    }
                    pos += bytes as usize;
                    sizes.push(sz);
                    last_size -= bytes + sz;
                }
                if last_size < 0 {
                    return Err(Error::InvalidPacket("invalid packet"));
                }
            } else if !self_delimited {
                last_size = len / count as i32;
                if last_size * count as i32 != len {
                    return Err(Error::InvalidPacket("invalid packet"));
                }
                for _ in 0..count - 1 {
                    sizes.push(last_size);
                }
            }
            count
        }
    };

    if self_delimited {
        let (bytes, sz) = parse_size(&data[pos..]);
        if bytes < 0 {
            return Err(Error::InvalidPacket("invalid packet"));
        }
        len -= bytes;
        if sz < 0 || sz > len {
            return Err(Error::InvalidPacket("invalid packet"));
        }
        pos += bytes as usize;
        if cbr {
            if sz * count as i32 > len {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            sizes.clear();
            for _ in 0..count - 1 {
                sizes.push(sz);
            }
            sizes.push(sz);
        } else {
            if bytes + sz > last_size {
                return Err(Error::InvalidPacket("invalid packet"));
            }
            sizes.push(sz);
        }
    } else {
        if last_size > 1275 {
            return Err(Error::InvalidPacket("invalid packet"));
        }
        sizes.push(last_size);
    }

    // Frame byte-ranges start at `pos`.
    let mut frames = Vec::with_capacity(count);
    let mut off = pos;
    for &s in &sizes {
        if off + s as usize > data.len() {
            return Err(Error::InvalidPacket("invalid packet"));
        }
        frames.push((off, s as usize));
        off += s as usize;
    }
    let packet_offset = off; // for self-delimited multistream advancement
    Ok((toc, frames, packet_offset))
}

/// Take one self-delimited packet from the front of `data` and re-emit it as a
/// normal Opus packet, also returning how many bytes it consumed.
///
/// Strip self-delimited framing into `out` directly without reallocating frames.
pub(crate) fn take_self_delimited_into(data: &[u8], out: &mut Vec<u8>) -> Result<usize> {
    let (toc, frames, consumed) = parse_packet(data, true)?;
    let count = frames.len();
    out.clear();
    if count == 1 {
        out.push(toc & 0xfc); // code 0
        let (o, l) = frames[0];
        out.extend_from_slice(&data[o..o + l]);
    } else if count == 2 && frames[0].1 == frames[1].1 {
        out.push((toc & 0xfc) | 0x1); // code 1
        for &(o, l) in &frames {
            out.extend_from_slice(&data[o..o + l]);
        }
    } else if count == 2 {
        out.push((toc & 0xfc) | 0x2); // code 2
        encode_size(frames[0].1 as i32, out);
        for &(o, l) in &frames {
            out.extend_from_slice(&data[o..o + l]);
        }
    } else {
        // Code 3
        let vbr = frames.iter().any(|&(_, l)| l != frames[0].1);
        if vbr {
            out.push((toc & 0xfc) | 0x3);
            out.push((count as u8) | 0x80);
            for &(_, l) in frames.iter().take(count - 1) {
                encode_size(l as i32, out);
            }
        } else {
            out.push((toc & 0xfc) | 0x3);
            out.push(count as u8);
        }
        for &(o, l) in &frames {
            out.extend_from_slice(&data[o..o + l]);
        }
    }
    Ok(consumed)
}

/// A multistream packet concatenates its streams in self-delimited form
/// (RFC 6716 Appendix B): every stream but the last carries an explicit length
/// for its final frame. [`OpusDecoder`](crate::OpusDecoder) parses only normal
/// packets, so that prefix has to be removed — handing the self-delimited bytes
/// straight through makes the decoder read the length as payload.
#[allow(dead_code)]
pub(crate) fn take_self_delimited(data: &[u8], _framesize: i32) -> Result<(Vec<u8>, usize)> {
    let mut out = Vec::new();
    let consumed = take_self_delimited_into(data, &mut out)?;
    Ok((out, consumed))
}

/// opus_repacketizer: accumulate frames from one or more same-config packets,
/// then emit them as a single re-framed packet.
#[derive(Default)]
pub struct Repacketizer {
    toc: u8,
    framesize: i32,
    frames: Vec<Vec<u8>>,
    /// Frame buffers [`clear`](Repacketizer::clear) has taken back, ready for
    /// [`cat`](Repacketizer::cat) to fill again. A repacketizer reused per
    /// packet — which is what a jitter buffer and the multistream encoder both
    /// do — would otherwise allocate one `Vec` per frame, for ever.
    spare: Vec<Vec<u8>>,
}

/// Shows what the repacketizer holds without printing the frames themselves,
/// which are compressed audio and unreadable either way.
impl std::fmt::Debug for Repacketizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repacketizer")
            .field("nb_frames", &self.frames.len())
            .field("toc", &format_args!("{:#04x}", self.toc))
            .field("framesize", &self.framesize)
            .field("bytes", &self.frames.iter().map(Vec::len).sum::<usize>())
            .finish()
    }
}

impl Repacketizer {
    /// An empty repacketizer, holding no frames.
    pub fn new() -> Self {
        Repacketizer::default()
    }

    /// Drop every frame, keeping the buffers that held them.
    ///
    /// This is what makes one repacketizer reusable across packets. Building a
    /// fresh [`Repacketizer::new`] per packet is correct but allocates a buffer
    /// per frame every time; clearing and refilling one reuses them.
    pub fn clear(&mut self) {
        self.spare.append(&mut self.frames);
        self.toc = 0;
        self.framesize = 0;
    }

    /// Frames accumulated so far by [`cat`](Self::cat).
    ///
    /// Also the upper bound for [`out_range`](Self::out_range), which is how a
    /// packet is split rather than joined.
    pub fn nb_frames(&self) -> usize {
        self.frames.len()
    }

    /// Append the frames of `data` (opus_repacketizer_cat). Errors if the TOC
    /// config differs from frames already held, or the 120 ms cap is exceeded.
    pub fn cat(&mut self, data: &[u8]) -> Result<()> {
        self.cat_impl(data, false)
    }

    fn cat_impl(&mut self, data: &[u8], self_delimited: bool) -> Result<()> {
        if data.is_empty() {
            return Err(Error::InvalidPacket("cat: the packet is empty"));
        }
        if self.frames.is_empty() {
            self.toc = data[0];
            self.framesize = samples_per_frame(data[0], 8000);
        } else if self.toc & 0xfc != data[0] & 0xfc {
            return Err(Error::InvalidPacket("toc mismatch"));
        }
        let curr = frame_count(data)?;
        if (curr + self.frames.len()) as i32 * self.framesize > 960 {
            return Err(Error::InvalidPacket("packet exceeds 120 ms"));
        }
        let (_toc, ranges, _off) = parse_packet(data, self_delimited)?;
        for (o, l) in ranges {
            let mut frame = self.spare.pop().unwrap_or_default();
            frame.clear();
            frame.extend_from_slice(&data[o..o + l]);
            self.frames.push(frame);
        }
        Ok(())
    }

    /// Emit frames [begin, end) as one packet (opus_repacketizer_out_range).
    pub fn out_range(&self, begin: usize, end: usize) -> Result<Vec<u8>> {
        self.out_range_impl(begin, end, None)
    }

    /// Emit all held frames (opus_repacketizer_out).
    pub fn out(&self) -> Result<Vec<u8>> {
        self.out_range_impl(0, self.frames.len(), None)
    }

    /// [`out`](Self::out), appended to `out` instead of allocating.
    ///
    /// The `_into` forms exist because every other one hands back a fresh
    /// `Vec`, which is an allocation per packet a caller in a steady loop can
    /// do without. They append, so one buffer can accumulate several packets.
    pub fn out_into(&self, out: &mut Vec<u8>) -> Result<()> {
        self.out_range_full(0, self.frames.len(), None, false, out)
    }

    /// [`out_range`](Self::out_range), appended to `out`.
    pub fn out_range_into(&self, begin: usize, end: usize, out: &mut Vec<u8>) -> Result<()> {
        self.out_range_full(begin, end, None, false, out)
    }

    /// [`out_self_delimited`](Self::out_self_delimited), appended to `out`.
    pub fn out_self_delimited_into(&self, out: &mut Vec<u8>) -> Result<()> {
        self.out_range_full(0, self.frames.len(), None, true, out)
    }

    /// `opus_repacketizer_out_range_impl`. `pad_to` is C's `pad` flag plus its
    /// `maxlen`: padding forces the code 3 framing that can carry it.
    pub(crate) fn out_range_impl(
        &self,
        begin: usize,
        end: usize,
        pad_to: Option<usize>,
    ) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.out_range_full(begin, end, pad_to, false, &mut out)?;
        Ok(out)
    }

    /// Emit all frames with the self-delimited framing multistream uses (the
    /// last frame's length is coded so the packet's total size is derivable).
    pub fn out_self_delimited(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.out_range_full(0, self.frames.len(), None, true, &mut out)?;
        Ok(out)
    }

    /// Appends, so `out` may already hold something; `start` is where this
    /// packet begins inside it, and every length below is measured from there.
    fn out_range_full(
        &self,
        begin: usize,
        end: usize,
        pad_to: Option<usize>,
        self_delimited: bool,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        if begin >= end || end > self.frames.len() {
            return Err(Error::InvalidArgument(
                "frame range must satisfy begin < end <= nb_frames",
            ));
        }
        let count = end - begin;
        let lens: Vec<usize> = self.frames[begin..end].iter().map(|f| f.len()).collect();
        let start = out.len();

        if count == 1 {
            out.push(self.toc & 0xfc); // code 0
        } else if count == 2 && lens[0] == lens[1] {
            out.push((self.toc & 0xfc) | 0x1); // code 1
        } else if count == 2 {
            out.push((self.toc & 0xfc) | 0x2); // code 2
            encode_size(lens[0] as i32, out);
        }

        let want_pad = pad_to.is_some();
        if count > 2 || want_pad {
            // Code 3 (needed for >2 frames, or to carry padding). Discards
            // whatever the branch above wrote for *this* packet, not anything
            // already in the caller's buffer.
            out.truncate(start);
            let vbr = lens.iter().any(|&l| l != lens[0]);
            if vbr {
                out.push((self.toc & 0xfc) | 0x3);
                out.push((count as u8) | 0x80);
            } else {
                out.push((self.toc & 0xfc) | 0x3);
                out.push(count as u8);
            }
            // Compute current size to know the padding amount.
            let mut tot = 2usize;
            if vbr {
                for &l in lens.iter().take(count - 1) {
                    tot += 1 + usize::from(l >= 252) + l;
                }
                tot += lens[count - 1];
            } else {
                tot += count * lens[0];
            }
            let pad_amount = pad_to.map(|n| n.saturating_sub(tot)).unwrap_or(0);
            if pad_amount != 0 {
                out[1] |= 0x40; // padding flag
                let nb_255s = (pad_amount - 1) / 255;
                out.extend(std::iter::repeat_n(255u8, nb_255s));
                out.push((pad_amount - 255 * nb_255s - 1) as u8);
            }
            if vbr {
                for &l in lens.iter().take(count - 1) {
                    encode_size(l as i32, out);
                }
            }
            if self_delimited {
                encode_size(lens[count - 1] as i32, out);
            }
            for f in &self.frames[begin..end] {
                out.extend_from_slice(f);
            }
            if let Some(n) = pad_to {
                while out.len() - start < n {
                    out.push(0);
                }
            }
            return Ok(());
        }

        if self_delimited {
            encode_size(lens[count - 1] as i32, out);
        }
        for f in &self.frames[begin..end] {
            out.extend_from_slice(f);
        }
        Ok(())
    }
}

/// opus_packet_pad: grow `packet` in place to `new_len` bytes by adding opus
/// padding (no re-encode). No-op if already `new_len`; errors if `new_len` is
/// smaller.
pub fn pad_packet(packet: &mut Vec<u8>, new_len: usize) -> Result<()> {
    if packet.is_empty() {
        return Err(Error::InvalidArgument("pad_packet: the packet is empty"));
    }
    if packet.len() == new_len {
        return Ok(());
    }
    if packet.len() > new_len {
        return Err(Error::InvalidArgument(
            "pad_packet: new_len is smaller than the packet",
        ));
    }
    let mut rp = Repacketizer::new();
    rp.cat(packet)?;
    let padded = rp.out_range_impl(0, rp.nb_frames(), Some(new_len))?;
    *packet = padded;
    Ok(())
}

/// opus_packet_unpad: strip opus padding, returning the minimal packet.
pub fn unpad_packet(packet: &[u8]) -> Result<Vec<u8>> {
    if packet.is_empty() {
        return Err(Error::InvalidArgument("unpad_packet: the packet is empty"));
    }
    let mut rp = Repacketizer::new();
    rp.cat(packet)?;
    rp.out_range_impl(0, rp.nb_frames(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a synthetic code-3 VBR packet with 3 frames of distinct lengths,
    // split via out_range, and re-merge -> byte-identical (round-trip fidelity).
    #[test]
    fn split_merge_roundtrip() {
        // toc config 12 (hybrid SWB 10ms) stereo bit off, code 3.
        let toc = 12u8 << 3;
        let mut pkt = vec![toc | 0x3, 3 | 0x80]; // code 3, vbr, count 3
        let f0 = vec![0xAAu8; 3];
        let f1 = vec![0xBBu8; 5];
        let f2 = vec![0xCCu8; 4];
        encode_size(3, &mut pkt);
        encode_size(5, &mut pkt);
        pkt.extend_from_slice(&f0);
        pkt.extend_from_slice(&f1);
        pkt.extend_from_slice(&f2);

        let mut rp = Repacketizer::new();
        rp.cat(&pkt).unwrap();
        assert_eq!(rp.nb_frames(), 3);
        // out() must reproduce the exact same packet.
        assert_eq!(rp.out().unwrap(), pkt);
        // Splitting single frames yields code-0 packets with the frame bytes.
        let s0 = rp.out_range(0, 1).unwrap();
        assert_eq!(s0[0] & 0x3, 0);
        assert_eq!(&s0[1..], &f0[..]);
        let s1 = rp.out_range(1, 2).unwrap();
        assert_eq!(&s1[1..], &f1[..]);
    }

    #[test]
    fn pad_unpad_identity() {
        let toc = 8u8 << 3; // silk WB code 0
        let mut pkt = vec![toc];
        pkt.extend_from_slice(&[1, 2, 3, 4, 5]);
        let orig = pkt.clone();
        pad_packet(&mut pkt, orig.len() + 10).unwrap();
        assert_eq!(pkt.len(), orig.len() + 10);
        let back = unpad_packet(&pkt).unwrap();
        // frame bytes recovered
        let (_t, f, _) = parse_packet(&back, false).unwrap();
        assert_eq!(&back[f[0].0..f[0].0 + f[0].1], &orig[1..]);
    }

    #[test]
    fn cbr_merge_code1() {
        // Two equal-length frames merge to code 1.
        let toc = 8u8 << 3;
        let p = vec![toc, 9, 9, 9]; // code 0, 3-byte frame
        let mut rp = Repacketizer::new();
        rp.cat(&p).unwrap();
        rp.cat(&p).unwrap();
        let out = rp.out().unwrap();
        assert_eq!(out[0] & 0x3, 1); // code 1 (equal sizes)
        assert_eq!(rp.nb_frames(), 2);
    }
}

#[cfg(test)]
mod sd_tests {
    use super::*;
    #[test]
    fn self_delimited_roundtrip() {
        // 3-frame vbr packet -> self-delimited -> parse(self_delimited) recovers frames.
        let toc = 12u8 << 3;
        let mut rp = Repacketizer::new();
        let mut p = vec![toc | 0x3, 3 | 0x80];
        encode_size(3, &mut p);
        encode_size(5, &mut p);
        p.extend_from_slice(&[1u8; 3]);
        p.extend_from_slice(&[2u8; 5]);
        p.extend_from_slice(&[3u8; 4]);
        rp.cat(&p).unwrap();
        let sd = rp.out_self_delimited().unwrap();
        // append trailing bytes to simulate concatenation; parse must stop at packet_offset
        let mut stream = sd.clone();
        stream.extend_from_slice(&[0xEE; 7]);
        let (t, frames, off) = parse_packet(&stream, true).unwrap();
        assert_eq!(t, toc | 0x3);
        assert_eq!(frames.len(), 3);
        assert_eq!(&stream[frames[0].0..frames[0].0 + frames[0].1], &[1, 1, 1]);
        assert_eq!(
            &stream[frames[2].0..frames[2].0 + frames[2].1],
            &[3, 3, 3, 3]
        );
        assert_eq!(off, sd.len()); // packet ends exactly at the SD boundary
    }
}
