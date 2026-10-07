use eframe::egui;

const NANUM_GOTHIC: &[u8] = include_bytes!("../../assets/fonts/NanumGothic-Regular.ttf");

/// egui 기본 폰트에는 한글이 없으므로 나눔고딕을 대체 폰트로 등록한다.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "nanum-gothic".into(),
        egui::FontData::from_static(NANUM_GOTHIC).into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("nanum-gothic".into());
    }
    ctx.set_fonts(fonts);
}
