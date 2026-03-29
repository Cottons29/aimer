

pub struct ScrollSpring {
    pub stiffness: f32,
}

impl ScrollSpring {
    pub fn new(stiffness: f32) -> Self {
        Self { stiffness }
    }

    /// Critically damped spring exact solution
    /// returns (new_x, new_v)
    pub fn update(&self, x0: f32, v0: f32, dt: f32) -> (f32, f32) {
        let omega = self.stiffness.sqrt();
        let c1 = x0;
        let c2 = v0 + omega * x0;

        let exp_term = (-omega * dt).exp();
        
        let x_next = (c1 + c2 * dt) * exp_term;
        let v_next = (c2 - omega * (c1 + c2 * dt)) * exp_term;

        (x_next, v_next)
    }
}
