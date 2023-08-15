
pub struct SigmaDelta {
    integrator: f32
}

impl SigmaDelta {
    pub fn new() -> Self {
        Self {
            integrator: 0.0
        }
    }

    pub fn put(&mut self, sample: f32) -> i32 {
        let feedback = self.integrator as i32;
        let added = sample - feedback as f32;
        self.integrator += added;
        self.integrator as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ones() {
        let mut sd = SigmaDelta::new();
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(1.0), 1);
    }

    #[test]
    fn whole_numbers() {
        let mut sd = SigmaDelta::new();
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(2.0), 2);
        assert_eq!(sd.put(3.0), 3);
        assert_eq!(sd.put(-2.0), -2);
        assert_eq!(sd.put(1.0), 1);
        assert_eq!(sd.put(2.0), 2);
        assert_eq!(sd.put(3.0), 3);
    }

    #[test]
    fn half() {
        let mut sd = SigmaDelta::new();
        assert_eq!(sd.put(0.5), 0);
        assert_eq!(sd.put(0.5), 1);
        assert_eq!(sd.put(0.5), 0);
        assert_eq!(sd.put(0.5), 1);
        assert_eq!(sd.put(0.5), 0);
        assert_eq!(sd.put(0.5), 1);
        assert_eq!(sd.put(0.5), 0);
        assert_eq!(sd.put(0.5), 1);
        assert_eq!(sd.put(0.5), 0);
    }
}
