use anyhow::{Context as _, Result, anyhow, bail};
use ff::format::sample::{Sample, Type};
use ff::{ChannelLayout, Dictionary, Packet, codec, frame};
use ffmpeg_next as ff;

/// Opus through FFmpeg's `libopus` encoder: 48 kHz stereo, 20 ms frames, low-delay application.
pub struct OpusEncoder {
    enc: ff::encoder::Audio,
    pts: i64,
}

// Raw FFmpeg pointers are owned by the encoder and used through `&mut self` only.
unsafe impl Send for OpusEncoder {}

const FRAME: usize = 960;

impl OpusEncoder {
    pub fn new(kbps: u32) -> Result<Self> {
        crate::ff::init();
        let codec = ff::encoder::find_by_name("libopus")
            .ok_or_else(|| anyhow!("FFmpeg build has no libopus encoder"))?;
        let ctx = codec::context::Context::new_with_codec(codec);
        let mut a = ctx.encoder().audio()?;
        a.set_rate(48000);
        a.set_channel_layout(ChannelLayout::STEREO);
        a.set_format(Sample::F32(Type::Packed));
        a.set_bit_rate(kbps.max(6) as usize * 1000);
        a.set_time_base((1, 48000));
        let mut o = Dictionary::new();
        o.set("application", "lowdelay");
        o.set("frame_duration", "20");
        o.set("vbr", "off");
        let enc = a.open_with(o).context("open libopus")?;
        Ok(OpusEncoder { enc, pts: 0 })
    }

    /// Encodes exactly 960 stereo frames (1920 interleaved f32) into one Opus packet.
    pub fn encode(&mut self, pcm_stereo_48k: &[f32]) -> Result<Vec<u8>> {
        if pcm_stereo_48k.len() != FRAME * 2 {
            bail!(
                "Opus frame must be {} f32 samples, got {}",
                FRAME * 2,
                pcm_stereo_48k.len()
            );
        }
        let mut fr = frame::Audio::new(Sample::F32(Type::Packed), FRAME, ChannelLayout::STEREO);
        fr.set_rate(48000);
        fr.set_pts(Some(self.pts));
        self.pts += FRAME as i64;
        for (dst, s) in fr
            .data_mut(0)
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pcm_stereo_48k)
        {
            dst.copy_from_slice(&s.to_ne_bytes());
        }
        self.enc.send_frame(&fr).context("opus send_frame")?;
        let mut pkt = Packet::empty();
        match self.enc.receive_packet(&mut pkt) {
            Ok(()) => Ok(pkt.data().unwrap_or_default().to_vec()),
            Err(e) => Err(e).context("opus receive_packet"),
        }
    }
}
