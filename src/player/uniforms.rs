#[derive(Clone)]
pub struct Uniforms {
    pub time: f32,
    pub flash: f32,
    pub effect_color: [f32; 3],
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            time: 0.0,
            flash: 0.0,
            effect_color: [1.0, 1.0, 1.0],
        }
    }
}
