/// Splits a multi-channel PCM stream into independent channel pair buffers.
/// Mirrors MediaInfoLib's File_ChannelSplitting for SMPTE ST 337 AES3.
pub struct ChannelSplitter {
    pub channels: Vec<SplitChannel>,
    pub channel_total: u8,
    pub bit_depth: u8,
    pub sampling_rate: u16,
}

pub struct SplitChannel {
    buffer: Vec<u8>,
    pub stream_index: usize,
}

impl ChannelSplitter {
    pub fn new(channel_total: u8, bit_depth: u8, sampling_rate: u16) -> Self {
        let pair_count = (channel_total as usize) / 2;
        ChannelSplitter {
            channels: (0..pair_count)
                .map(|i| SplitChannel { buffer: Vec::new(), stream_index: i })
                .collect(),
            channel_total,
            bit_depth,
            sampling_rate,
        }
    }

    /// Feed a frame of interleaved multi-channel PCM data.
    /// Deinterleaves into independent channel-pair buffers.
    pub fn feed_frame(&mut self, data: &[u8]) {
        let bytes_per_sample = (self.bit_depth as usize) / 8;
        let sample_count = data.len() / bytes_per_sample / self.channel_total as usize;
        for pair_idx in 0..self.channels.len() {
            for sample in 0..sample_count {
                let src_offset =
                    (sample * self.channel_total as usize + pair_idx * 2) * bytes_per_sample;
                self.channels[pair_idx]
                    .buffer
                    .extend_from_slice(&data[src_offset..src_offset + bytes_per_sample]);
                if pair_idx * 2 + 1 < self.channel_total as usize {
                    let src_offset2 = (sample * self.channel_total as usize + pair_idx * 2 + 1)
                        * bytes_per_sample;
                    self.channels[pair_idx]
                        .buffer
                        .extend_from_slice(&data[src_offset2..src_offset2 + bytes_per_sample]);
                }
            }
        }
    }

    pub fn get_buffers(&self) -> Vec<&[u8]> {
        self.channels.iter().map(|c| c.buffer.as_slice()).collect()
    }

    pub fn pair_count(&self) -> usize {
        self.channels.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_4ch_16bit() {
        // 4 channels, 16-bit: sample0 = [L0 R0 L1 R1]
        // channel_total=4, pair_count=2, bytes_per_sample=2
        let mut splitter = ChannelSplitter::new(4, 16, 48000);
        // Two samples interleaved across 4 channels: 2*4*2=16 bytes
        let data = vec![
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, // sample 0: L0,R0,L1,R1
            0x00, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00, 0x08, // sample 1: L0,R0,L1,R1
        ];
        splitter.feed_frame(&data);
        let bufs = splitter.get_buffers();
        assert_eq!(bufs.len(), 2);
        // Pair 0 (L0,R0): [0x0001, 0x0002, 0x0005, 0x0006]
        assert_eq!(bufs[0].len(), 8);
        // Pair 1 (L1,R1): [0x0003, 0x0004, 0x0007, 0x0008]
        assert_eq!(bufs[1].len(), 8);
    }

    #[test]
    fn test_2ch_no_split() {
        let mut splitter = ChannelSplitter::new(2, 24, 48000);
        assert_eq!(splitter.pair_count(), 1);
        let data = vec![0u8; 2 * 3]; // 1 sample, 2 channels, 24-bit = 6 bytes
        splitter.feed_frame(&data);
        assert_eq!(splitter.get_buffers()[0].len(), 6);
    }
}
