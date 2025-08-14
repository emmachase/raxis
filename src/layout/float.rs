pub const EPSILON: f32 = 1e-6;

pub trait EpsFloatCmp {
    fn eq_eps(&self, other: &Self) -> bool;
    fn lt_eps(&self, other: &Self) -> bool;
    fn gt_eps(&self, other: &Self) -> bool;
}

impl EpsFloatCmp for f32 {
    #[inline]
    fn eq_eps(&self, other: &Self) -> bool {
        (self - other).abs() < EPSILON
    }

    #[inline]
    fn lt_eps(&self, other: &Self) -> bool {
        other - self > EPSILON
    }

    #[inline]
    fn gt_eps(&self, other: &Self) -> bool {
        self - other > EPSILON
    }
}
