/// Merges independent mono PCM streams into one interleaved stream.
/// The reverse of channel splitting — mirrors MediaInfoLib's File_ChannelGrouping.
pub struct ChannelGrouper {
    pub channels: Vec<Vec<u8>>,
    pub channel_total: u8,
    pub bit_depth: u8,
    pub sampling_rate: u16,
}

impl ChannelGrouper {
    pub fn new(channel_total: u8, bit_depth: u8, sampling_rate: u16) -> Self {
        ChannelGrouper {
            channels: vec![Vec::new(); channel_total as usize],
            channel_total,
            bit_depth,
            sampling_rate,
        }
    }

    pub fn feed_channel(&mut self, channel_pos: u8, data: &[u8]) {
        if (channel_pos as usize) < self.channels.len() {
            self.channels[channel_pos as usize].extend_from_slice(data);
        }
    }

    /// Returns true if all channels have at least `min_bytes` of data.
    pub fn ready(&self, min_bytes: usize) -> bool {
        self.channels.iter().all(|c| c.len() >= min_bytes)
    }

    /// Interleave all channel buffers up to `samples` frames, removing consumed data.
    pub fn interleave(&mut self, frames: usize) -> Vec<u8> {
        let bytes_per_sample = self.bit_depth as usize / 8;
        let chunk = frames * bytes_per_sample;
        let mut output = Vec::with_capacity(chunk * self.channel_total as usize);
        for frame in 0..frames {
            for ch in 0..self.channel_total as usize {
                let start = frame * bytes_per_sample;
                output.extend_from_slice(&self.channels[ch][start..start + bytes_per_sample]);
            }
        }
        // Drain consumed bytes
        for ch in &mut self.channels {
            ch.drain(0..chunk);
        }
        output
    }

    pub fn total_channel_bytes(&self) -> usize {
        self.channels.iter().map(|c| c.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_group_two_channels_16bit() {
        let mut grouper = ChannelGrouper::new(2, 16, 48000);
        grouper.feed_channel(0, &[0x00, 0x01]);
        grouper.feed_channel(1, &[0x00, 0x02]);
        assert!(grouper.ready(2));
        let interleaved = grouper.interleave(1);
        // L then R: [0x00, 0x01, 0x00, 0x02]
        assert_eq!(interleaved, vec![0x00, 0x01, 0x00, 0x02]);
    }
}
