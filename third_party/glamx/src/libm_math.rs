//! dharmatiles' patch: the transcendental functions the macros call, as `libm`'s on every target.
//! std's `acos`/`cos`/`sin`/`atan2` are the platform's libm natively and compiler-builtins' in the browser,
//! and they round differently; parry's convex hull orders its first mesh by these eigenvalues.
pub trait LibmMath: Sized {
    fn lm_acos(self) -> Self;
    fn lm_cos(self) -> Self;
    fn lm_sin(self) -> Self;
    fn lm_atan2(self, x: Self) -> Self;
}
impl LibmMath for f32 {
    fn lm_acos(self) -> f32 {
        libm::acosf(self)
    }
    fn lm_cos(self) -> f32 {
        libm::cosf(self)
    }
    fn lm_sin(self) -> f32 {
        libm::sinf(self)
    }
    fn lm_atan2(self, x: f32) -> f32 {
        libm::atan2f(self, x)
    }
}
impl LibmMath for f64 {
    fn lm_acos(self) -> f64 {
        libm::acos(self)
    }
    fn lm_cos(self) -> f64 {
        libm::cos(self)
    }
    fn lm_sin(self) -> f64 {
        libm::sin(self)
    }
    fn lm_atan2(self, x: f64) -> f64 {
        libm::atan2(self, x)
    }
}
