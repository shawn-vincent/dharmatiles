//! Small 3-vector helpers on `[f64; 3]`, shared by the kernels and the kit
//! so each module does not grow its own copies.

/// `a − b`.
#[must_use]
pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a + b`.
#[must_use]
pub fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a × s`.
#[must_use]
pub fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `a · b`.
#[must_use]
pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a × b`.
#[must_use]
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `|a|`.
#[must_use]
pub fn norm(a: [f64; 3]) -> f64 {
    libm::sqrt(dot(a, a))
}

/// `|a − b|`.
#[must_use]
pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    norm(sub(a, b))
}

/// `a` scaled to unit length (`a` itself when it is zero).
#[must_use]
pub fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = norm(a);
    if l > 0.0 { a.map(|x| x / l) } else { a }
}
