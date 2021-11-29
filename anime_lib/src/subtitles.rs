/// Filters for SSA subtitles.
#[derive(Default)]
pub struct SSAFilter {
    pub layers: Vec<isize>,
    pub styles: Vec<String>,
}

impl SSAFilter {
    pub fn check(&self, entry: &substation::Entry) -> bool {
        (self.layers.is_empty()
            || entry
                .layer
                .as_ref()
                .map(|v| self.layers.contains(v))
                .unwrap_or(false))
            && (self.styles.is_empty()
                || entry
                    .style
                    .as_ref()
                    .map(|v| self.styles.contains(v))
                    .unwrap_or(false))
    }
}
