//! Small deterministic f64 geometry; no BLAS, clocks, random source, or native APIs.
pub(crate) type P2 = [f64; 2];
pub(crate) type V3 = [f64; 3];

pub(crate) fn add<const N: usize>(a: [f64; N], b: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn sub<const N: usize>(a: [f64; N], b: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn mul<const N: usize>(a: [f64; N], s: f64) -> [f64; N] {
    a.map(|v| v * s)
}
pub(crate) fn dot<const N: usize>(a: [f64; N], b: [f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub(crate) fn norm<const N: usize>(a: [f64; N]) -> f64 {
    dot(a, a).sqrt()
}
pub(crate) fn unit<const N: usize>(v: [f64; N], fallback: [f64; N]) -> [f64; N] {
    let n = norm(v);
    if n > 1e-12 {
        mul(v, 1.0 / n)
    } else {
        fallback
    }
}
pub(crate) fn lerp<const N: usize>(a: [f64; N], b: [f64; N], t: f64) -> [f64; N] {
    add(mul(a, 1.0 - t), mul(b, t))
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn cross2(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// An orthogonal fallback for a carbonyl parallel to the local backbone.
pub(crate) fn perpendicular(tangent: V3) -> V3 {
    let axis = (0..3)
        .min_by(|a, b| tangent[*a].abs().total_cmp(&tangent[*b].abs()))
        .unwrap_or(0);
    let mut v = [0.0; 3];
    v[axis] = 1.0;
    unit(sub(v, mul(tangent, dot(v, tangent))), [1.0, 0.0, 0.0])
}

pub(crate) fn gradient<const N: usize>(values: &[[f64; N]], i: usize) -> [f64; N] {
    if i == 0 {
        sub(values[1], values[0])
    } else if i + 1 == values.len() {
        sub(values[i], values[i - 1])
    } else {
        mul(sub(values[i + 1], values[i - 1]), 0.5)
    }
}

/// scipy.ndimage.gaussian_filter1d(mode="nearest", truncate=4).
pub(crate) fn gaussian<const N: usize>(values: &[[f64; N]], sigma: f64) -> Vec<[f64; N]> {
    let radius = (4.0 * sigma + 0.5) as isize;
    let weights: Vec<f64> = (-radius..=radius)
        .map(|i| (-0.5 * (i as f64 / sigma).powi(2)).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    (0..values.len())
        .map(|i| {
            let mut value = [0.0; N];
            for (j, w) in weights.iter().enumerate() {
                let k =
                    (i as isize + j as isize - radius).clamp(0, values.len() as isize - 1) as usize;
                value = add(value, mul(values[k], w / sum));
            }
            value
        })
        .collect()
}

/// Uniform-knot not-a-knot cubic spline, equivalent to SciPy's CubicSpline
/// default boundary condition for n >= 4. Solve second derivatives in O(n).
pub(crate) struct Cubic<const N: usize> {
    values: Vec<[f64; N]>,
    second: Vec<[f64; N]>,
}
impl<const N: usize> Cubic<N> {
    pub(crate) fn new(values: Vec<[f64; N]>) -> Self {
        let n = values.len();
        let m = n - 2;
        let mut diag = vec![4.0; m];
        let mut upper = vec![1.0; m];
        let mut rhs: Vec<_> = (1..n - 1)
            .map(|i| {
                mul(
                    add(sub(values[i - 1], mul(values[i], 2.0)), values[i + 1]),
                    6.0,
                )
            })
            .collect();
        diag[0] = 6.0;
        diag[m - 1] = 6.0;
        upper[0] = 0.0;
        for i in 1..m {
            let lower = if i + 1 == m { 0.0 } else { 1.0 };
            let ratio = lower / diag[i - 1];
            diag[i] -= ratio * upper[i - 1];
            rhs[i] = sub(rhs[i], mul(rhs[i - 1], ratio));
        }
        let mut second = vec![[0.0; N]; n];
        second[n - 2] = mul(rhs[m - 1], 1.0 / diag[m - 1]);
        for j in (0..m - 1).rev() {
            second[j + 1] = mul(sub(rhs[j], mul(second[j + 2], upper[j])), 1.0 / diag[j]);
        }
        second[0] = sub(mul(second[1], 2.0), second[2]);
        second[n - 1] = sub(mul(second[n - 2], 2.0), second[n - 3]);
        Self { values, second }
    }
    pub(crate) fn at(&self, t: f64) -> ([f64; N], [f64; N]) {
        let i = (t.floor() as usize).min(self.values.len() - 2);
        let b = (t - i as f64).clamp(0.0, 1.0);
        let a = 1.0 - b;
        let correction = add(
            mul(self.second[i], (a * a * a - a) / 6.0),
            mul(self.second[i + 1], (b * b * b - b) / 6.0),
        );
        let value = add(lerp(self.values[i], self.values[i + 1], b), correction);
        let derivative = add(
            sub(self.values[i + 1], self.values[i]),
            add(
                mul(self.second[i], (1.0 - 3.0 * a * a) / 6.0),
                mul(self.second[i + 1], (3.0 * b * b - 1.0) / 6.0),
            ),
        );
        (value, derivative)
    }
}

/// Shape-preserving cubic Hermite interpolation (PCHIP) at unit-spaced knots.
pub(crate) struct Pchip {
    values: Vec<f64>,
    slopes: Vec<f64>,
}
impl Pchip {
    pub(crate) fn new(values: Vec<f64>) -> Self {
        let n = values.len();
        let delta: Vec<_> = values.windows(2).map(|v| v[1] - v[0]).collect();
        let endpoint = |a: f64, b: f64| {
            let d = (3.0 * a - b) * 0.5;
            if d.signum() != a.signum() {
                0.0
            } else if a.signum() != b.signum() && d.abs() > 3.0 * a.abs() {
                3.0 * a
            } else {
                d
            }
        };
        let mut slopes = vec![0.0; n];
        slopes[0] = endpoint(delta[0], delta[1]);
        slopes[n - 1] = endpoint(delta[n - 2], delta[n - 3]);
        for i in 1..n - 1 {
            let (a, b) = (delta[i - 1], delta[i]);
            if a * b > 0.0 {
                slopes[i] = 2.0 * a * b / (a + b);
            }
        }
        Self { values, slopes }
    }
    pub(crate) fn at(&self, t: f64) -> f64 {
        let i = (t.floor() as usize).min(self.values.len() - 2);
        let s = (t - i as f64).clamp(0.0, 1.0);
        let s2 = s * s;
        let s3 = s2 * s;
        (2.0 * s3 - 3.0 * s2 + 1.0) * self.values[i]
            + (s3 - 2.0 * s2 + s) * self.slopes[i]
            + (-2.0 * s3 + 3.0 * s2) * self.values[i + 1]
            + (s3 - s2) * self.slopes[i + 1]
    }
}

#[derive(Clone)]
pub(crate) struct Camera {
    pub(crate) center: V3,
    pub(crate) basis: [V3; 3],
}
impl Camera {
    /// Jacobi diagonalization of symmetric 3x3 covariance. Major principal
    /// axis is vertical; signs are canonicalized for deterministic previews.
    pub(crate) fn new(points: &[V3], angles: V3) -> Self {
        let center = mul(
            points.iter().copied().fold([0.0; 3], add),
            1.0 / points.len() as f64,
        );
        let mut a = [[0.0; 3]; 3];
        for p in points {
            let v = sub(*p, center);
            for i in 0..3 {
                for j in 0..3 {
                    a[i][j] += v[i] * v[j];
                }
            }
        }
        let mut vectors = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for _ in 0..40 {
            let (p, q) = [(0, 1), (0, 2), (1, 2)]
                .into_iter()
                .max_by(|(p, q), (r, s)| a[*p][*q].abs().total_cmp(&a[*r][*s].abs()))
                .unwrap_or((0, 1));
            if a[p][q].abs() < 1e-14 * (a[0][0].abs() + a[1][1].abs() + a[2][2].abs()).max(1e-20) {
                break;
            }
            let theta = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
            let (s, c) = theta.sin_cos();
            let (app, aqq, apq) = (a[p][p], a[q][q], a[p][q]);
            a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
            a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
            a[p][q] = 0.0;
            a[q][p] = 0.0;
            for k in 0..3 {
                if k != p && k != q {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[p][k] = a[k][p];
                    a[k][q] = s * akp + c * akq;
                    a[q][k] = a[k][q];
                }
                let (vkp, vkq) = (vectors[k][p], vectors[k][q]);
                vectors[k][p] = c * vkp - s * vkq;
                vectors[k][q] = s * vkp + c * vkq;
            }
        }
        let mut order = [0, 1, 2];
        order.sort_by(|i, j| a[*j][*j].total_cmp(&a[*i][*i]));
        let axis = |i: usize| {
            let mut v = [vectors[0][i], vectors[1][i], vectors[2][i]];
            let k = (0..3)
                .max_by(|x, y| v[*x].abs().total_cmp(&v[*y].abs()))
                .unwrap_or(0);
            if v[k] < 0.0 {
                v = mul(v, -1.0);
            }
            v
        };
        let x = axis(order[1]);
        let y = axis(order[0]);
        Self {
            center,
            basis: [x, y, cross(x, y)],
        }
        .rotated(angles)
    }
    pub(crate) fn rotated(&self, angles: V3) -> Self {
        let center = self.center;
        let mut basis = self.basis;
        // Extrinsic xyz Euler rotation, Rz * Ry * Rx as in scipy Rotation.
        for (axis, angle) in angles.into_iter().enumerate() {
            let (s, c) = angle.to_radians().sin_cos();
            let old = basis;
            match axis {
                0 => {
                    basis[1] = sub(mul(old[1], c), mul(old[2], s));
                    basis[2] = add(mul(old[1], s), mul(old[2], c));
                }
                1 => {
                    basis[0] = add(mul(old[0], c), mul(old[2], s));
                    basis[2] = sub(mul(old[2], c), mul(old[0], s));
                }
                _ => {
                    basis[0] = sub(mul(old[0], c), mul(old[1], s));
                    basis[1] = add(mul(old[0], s), mul(old[1], c));
                }
            }
        }
        Self { center, basis }
    }
    pub(crate) fn vector(&self, v: V3) -> V3 {
        self.basis.map(|axis| dot(axis, v))
    }
    pub(crate) fn point(&self, p: V3) -> V3 {
        self.vector(sub(p, self.center))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn not_a_knot_reproduces_cubic_and_derivative() {
        let spline = Cubic::new(
            (0..8)
                .map(|x| [(x as f64).powi(3) - 2.0 * x as f64])
                .collect(),
        );
        for i in 0..701 {
            let t = i as f64 / 100.0;
            let (p, d) = spline.at(t);
            assert!((p[0] - (t.powi(3) - 2.0 * t)).abs() < 1e-10);
            assert!((d[0] - (3.0 * t * t - 2.0)).abs() < 1e-10);
        }
    }
    #[test]
    fn pchip_does_not_overshoot_arrow_widths() {
        let values = vec![0.08, 1.35, 1.35, 2.0, 0.06, 0.2, 0.08];
        let p = Pchip::new(values.clone());
        for i in 0..600 {
            let t = i as f64 / 100.0;
            let j = t.floor() as usize;
            assert!(p.at(t) >= values[j].min(values[j + 1]) - 1e-12);
            assert!(p.at(t) <= values[j].max(values[j + 1]) + 1e-12);
        }
    }
    #[test]
    fn pca_places_major_axis_vertically_and_is_orthonormal() {
        let points: Vec<_> = (-10..=10)
            .map(|i| [i as f64, 2.0 * i as f64, 3.0 * i as f64])
            .collect();
        let camera = Camera::new(&points, [0.0; 3]);
        let p = camera.point(points[0]);
        assert!(p[0].abs() < 1e-9 && p[2].abs() < 1e-9 && p[1].abs() > 30.0);
        for i in 0..3 {
            for j in 0..3 {
                assert!((dot(camera.basis[i], camera.basis[j]) - f64::from(i == j)).abs() < 1e-10);
            }
        }
        assert!(dot(cross(camera.basis[0], camera.basis[1]), camera.basis[2]) > 0.999);
    }
}
