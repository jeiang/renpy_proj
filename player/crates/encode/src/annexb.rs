//! Annex B helpers: NAL splitting, parameter-set repair, SPS inspection.

/// Splits an Annex B buffer into NAL units (start codes removed).
pub fn split_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new(); // (start code position, payload position)
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let sc = if i > 0 && data[i - 1] == 0 { i - 1 } else { i };
            starts.push((sc, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &(_, p)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(data.len(), |n| n.0);
        if p < end {
            out.push(&data[p..end]);
        }
    }
    out
}

/// NAL unit type (low five bits of the header byte).
pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map_or(0, |b| b & 0x1f)
}

/// True if the buffer starts with an Annex B start code.
pub fn is_annexb(data: &[u8]) -> bool {
    data.starts_with(&[0, 0, 1]) || data.starts_with(&[0, 0, 0, 1])
}

/// Converts length-prefixed (AVCC) NAL units to Annex B.
pub fn avcc_to_annexb(data: &[u8], length_size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut i = 0;
    while i + length_size <= data.len() {
        let mut n = 0usize;
        for b in &data[i..i + length_size] {
            n = (n << 8) | *b as usize;
        }
        i += length_size;
        if n == 0 || i + n > data.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[i..i + n]);
        i += n;
    }
    out
}

/// Parses an `avcC` record (codec extradata): returns (length size, SPS/PPS as Annex B).
pub fn parse_avcc(extra: &[u8]) -> Option<(usize, Vec<u8>)> {
    if extra.len() < 7 || extra[0] != 1 {
        return None;
    }
    let length_size = (extra[4] & 3) as usize + 1;
    let mut out = Vec::new();
    let mut p = 5;
    let nsps = (extra[p] & 0x1f) as usize;
    p += 1;
    for _ in 0..nsps {
        let n = u16::from_be_bytes([*extra.get(p)?, *extra.get(p + 1)?]) as usize;
        p += 2;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(extra.get(p..p + n)?);
        p += n;
    }
    let npps = *extra.get(p)? as usize;
    p += 1;
    for _ in 0..npps {
        let n = u16::from_be_bytes([*extra.get(p)?, *extra.get(p + 1)?]) as usize;
        p += 2;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(extra.get(p..p + n)?);
        p += n;
    }
    Some((length_size, out))
}

/// `profile_idc, constraint flags, level_idc` of the first SPS in an Annex B buffer
/// (the three bytes of the SDP `profile-level-id`).
pub fn profile_level_id(data: &[u8]) -> Option<[u8; 3]> {
    split_nals(data)
        .into_iter()
        .find(|n| nal_type(n) == 7 && n.len() >= 4)
        .map(|n| [n[1], n[2], n[3]])
}

/// Keeps the SPS/PPS seen so far and repairs IDR access units that lack them.
#[derive(Default)]
pub struct ParamSets {
    cache: Vec<u8>, // Annex B SPS + PPS
}

impl ParamSets {
    pub fn seed(&mut self, annexb_params: &[u8]) {
        if !annexb_params.is_empty() {
            self.cache = annexb_params.to_vec();
        }
    }

    /// Returns `(annexb, is_idr)`; prepends cached SPS/PPS to an IDR that has none.
    pub fn process(&mut self, data: Vec<u8>) -> (Vec<u8>, bool) {
        let nals = split_nals(&data);
        let idr = nals.iter().any(|n| nal_type(n) == 5);
        let has_sps = nals.iter().any(|n| nal_type(n) == 7);
        let has_pps = nals.iter().any(|n| nal_type(n) == 8);
        if has_sps && has_pps {
            let mut c = Vec::new();
            for n in nals.iter().filter(|n| matches!(nal_type(n), 7 | 8)) {
                c.extend_from_slice(&[0, 0, 0, 1]);
                c.extend_from_slice(n);
            }
            self.cache = c;
        }
        if idr && !(has_sps && has_pps) && !self.cache.is_empty() {
            let mut v = Vec::with_capacity(self.cache.len() + data.len());
            v.extend_from_slice(&self.cache);
            v.extend_from_slice(&data);
            return (v, true);
        }
        (data, idr)
    }
}
