/// 에뮬레이션할 기기.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Model {
    /// 카트리지 헤더의 CGB 플래그로 고른다.
    #[default]
    Auto,
    Dmg,
    Cgb,
}

impl Model {
    /// `Auto`를 헤더의 CGB 플래그(0x0143)로 확정한다. 0x80, 0xC0이면 CGB다.
    pub fn resolve(self, cgb_flag: u8) -> Model {
        match self {
            Model::Auto if matches!(cgb_flag, 0x80 | 0xC0) => Model::Cgb,
            Model::Auto => Model::Dmg,
            explicit => explicit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_picks_cgb_for_cgb_flags() {
        assert_eq!(Model::Auto.resolve(0x80), Model::Cgb);
        assert_eq!(Model::Auto.resolve(0xC0), Model::Cgb);
    }

    #[test]
    fn auto_picks_dmg_otherwise() {
        assert_eq!(Model::Auto.resolve(0x00), Model::Dmg);
        assert_eq!(Model::Auto.resolve(0x40), Model::Dmg);
    }

    #[test]
    fn explicit_model_wins_over_header() {
        assert_eq!(Model::Dmg.resolve(0xC0), Model::Dmg);
        assert_eq!(Model::Cgb.resolve(0x00), Model::Cgb);
    }
}
