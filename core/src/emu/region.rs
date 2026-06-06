#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Ntsc,
    Pal,
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Region::Ntsc => write!(f, "NTSC"),
            Region::Pal => write!(f, "PAL"),
        }
    }
}

impl Region {
    pub fn config(self) -> RegionConfig {
        match self {
            Region::Ntsc => RegionConfig {
                region: Region::Ntsc,
                master_clocks_per_cpu: 12,
                master_clocks_per_ppu: 4,
                num_scanlines: 262,
                pre_render_scanline: 261,
                vblank_scanline: 241,
                cpu_clock_rate: 1_789_773.0,
                frame_duration_nanos: 16_639_267,
                odd_frame_skip: true,
                input_poll_interval: 1790,
            },
            Region::Pal => RegionConfig {
                region: Region::Pal,
                master_clocks_per_cpu: 16,
                master_clocks_per_ppu: 5,
                num_scanlines: 312,
                pre_render_scanline: 311,
                vblank_scanline: 241,
                cpu_clock_rate: 1_662_607.0,
                frame_duration_nanos: 19_997_200,
                odd_frame_skip: false,
                input_poll_interval: 1663,
            },
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Region::Ntsc => 0,
            Region::Pal => 1,
        }
    }

    pub fn from_byte(b: u8) -> Option<Region> {
        match b {
            0 => Some(Region::Ntsc),
            1 => Some(Region::Pal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionConfig {
    pub region: Region,
    pub master_clocks_per_cpu: u64,
    pub master_clocks_per_ppu: u64,
    pub num_scanlines: u16,
    pub pre_render_scanline: u16,
    pub vblank_scanline: u16,
    pub cpu_clock_rate: f64,
    pub frame_duration_nanos: u64,
    pub odd_frame_skip: bool,
    pub input_poll_interval: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntsc_config() {
        let c = Region::Ntsc.config();
        assert_eq!(c.master_clocks_per_cpu, 12);
        assert_eq!(c.master_clocks_per_ppu, 4);
        assert_eq!(c.num_scanlines, 262);
        assert_eq!(c.pre_render_scanline, 261);
        assert!(c.odd_frame_skip);
        assert_eq!(c.master_clocks_per_cpu / c.master_clocks_per_ppu, 3);
    }

    #[test]
    fn test_pal_config() {
        let c = Region::Pal.config();
        assert_eq!(c.master_clocks_per_cpu, 16);
        assert_eq!(c.master_clocks_per_ppu, 5);
        assert_eq!(c.num_scanlines, 312);
        assert_eq!(c.pre_render_scanline, 311);
        assert!(!c.odd_frame_skip);
    }

    #[test]
    fn test_pal_ppu_cpu_ratio() {
        let c = Region::Pal.config();
        let ppu_dots_per_5_cpu = 5 * c.master_clocks_per_cpu / c.master_clocks_per_ppu;
        assert_eq!(ppu_dots_per_5_cpu, 16);
    }

    #[test]
    fn test_region_byte_roundtrip() {
        assert_eq!(
            Region::from_byte(Region::Ntsc.to_byte()),
            Some(Region::Ntsc)
        );
        assert_eq!(Region::from_byte(Region::Pal.to_byte()), Some(Region::Pal));
        assert_eq!(Region::from_byte(255), None);
    }
}
