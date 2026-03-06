pub trait ColorMixer {
    fn to_u32(&self) -> u32;
    fn to_css_color(&self) -> String {
        let c = self.to_u32();
        let a = ((c >> 24) & 0xFF) as f64 / 255.0;
        let r = (c >> 16) & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = c & 0xFF;

        format!("rgba({}, {}, {}, {})", r, g, b, a)
    }
}
