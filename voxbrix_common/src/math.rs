use serde::{
    Deserialize,
    Serialize,
};
use std::{
    borrow::Borrow,
    fmt::Debug,
    mem,
    ops::{
        Add,
        Index,
        IndexMut,
        Mul,
        Neg,
        Sub,
    },
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub struct Vec3<T>([T; 3]);

impl<T> Vec3<T> {
    pub const fn new(new: [T; 3]) -> Self {
        Self(new)
    }
}

impl<T> AsRef<[T; 3]> for Vec3<T> {
    fn as_ref(&self) -> &[T; 3] {
        &self.0
    }
}

impl<T> Index<usize> for Vec3<T> {
    type Output = T;

    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl<T> IndexMut<usize> for Vec3<T> {
    fn index_mut(&mut self, idx: usize) -> &mut T {
        &mut self.0[idx]
    }
}

impl<T> Mul<T> for Vec3<T>
where
    T: Mul<Output = T> + Copy,
{
    type Output = Vec3<T>;

    fn mul(self, other: T) -> Self::Output {
        Self(self.0.map(|v| v * other))
    }
}

impl<T> Neg for Vec3<T>
where
    T: Neg<Output = T> + Copy,
{
    type Output = Vec3<T>;

    fn neg(self) -> Self::Output {
        Self([-self[0], -self[1], -self[2]])
    }
}

impl<T> Add<Self> for Vec3<T>
where
    T: Add<Output = T> + Copy,
{
    type Output = Vec3<T>;

    fn add(self, other: Self) -> Self::Output {
        Self([self[0] + other[0], self[1] + other[1], self[2] + other[2]])
    }
}

impl<T> Sub<Self> for Vec3<T>
where
    T: Sub<Output = T> + Copy,
{
    type Output = Vec3<T>;

    fn sub(self, other: Self) -> Self::Output {
        Self([self[0] - other[0], self[1] - other[1], self[2] - other[2]])
    }
}

impl Vec3<f32> {
    pub fn normalize(self) -> Option<Self> {
        let l = (self[0] * self[0] + self[1] * self[1] + self[2] * self[2]).sqrt();
        if l == 0.0 {
            return None;
        }
        Some(Self([self[0] / l, self[1] / l, self[2] / l]))
    }

    pub fn cross(self, other: Self) -> Self {
        Self([
            self[1] * other[2] - self[2] * other[1],
            self[2] * other[0] - self[0] * other[2],
            self[0] * other[1] - self[1] * other[0],
        ])
    }

    pub fn dot(self, other: Self) -> f32 {
        self[0] * other[0] + self[1] * other[1] + self[2] * other[2]
    }

    pub fn to_homogeneous(self) -> [f32; 4] {
        [self[0], self[1], self[2], 0.0]
    }
}

impl<T> From<[T; 3]> for Vec3<T> {
    fn from(from: [T; 3]) -> Self {
        Vec3(from)
    }
}

impl<T> From<Vec3<T>> for [T; 3] {
    fn from(from: Vec3<T>) -> Self {
        from.0
    }
}

impl<T> Vec3<T> {
    pub fn map<U, F>(self, f: F) -> Vec3<U>
    where
        F: FnMut(T) -> U,
    {
        Vec3(self.0.map(f))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub struct Vec4<T>([T; 4]);

impl<T> Vec4<T> {
    pub const fn new(new: [T; 4]) -> Self {
        Self(new)
    }
}

impl<T> AsRef<[T; 4]> for Vec4<T> {
    fn as_ref(&self) -> &[T; 4] {
        &self.0
    }
}

impl<T> AsRef<Vec4<T>> for [T; 4] {
    fn as_ref(&self) -> &Vec4<T> {
        unsafe { mem::transmute(self) }
    }
}

impl<T> AsMut<Vec4<T>> for [T; 4] {
    fn as_mut(&mut self) -> &mut Vec4<T> {
        unsafe { mem::transmute(self) }
    }
}

impl<T> Index<usize> for Vec4<T> {
    type Output = T;

    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl<T> IndexMut<usize> for Vec4<T> {
    fn index_mut(&mut self, idx: usize) -> &mut T {
        &mut self.0[idx]
    }
}

impl<T> Mul<T> for Vec4<T>
where
    T: Mul<Output = T> + Copy,
{
    type Output = Vec4<T>;

    fn mul(self, other: T) -> Self::Output {
        Self(self.0.map(|v| v * other))
    }
}

impl<T> Neg for Vec4<T>
where
    T: Neg<Output = T> + Copy,
{
    type Output = Vec4<T>;

    fn neg(self) -> Self::Output {
        Self([-self[0], -self[1], -self[2], -self[3]])
    }
}

impl<T> Add<Self> for Vec4<T>
where
    T: Add<Output = T> + Copy,
{
    type Output = Vec4<T>;

    fn add(self, other: Self) -> Self::Output {
        Self([
            self[0] + other[0],
            self[1] + other[1],
            self[2] + other[2],
            self[3] + other[3],
        ])
    }
}

impl<T> From<[T; 4]> for Vec4<T> {
    fn from(from: [T; 4]) -> Self {
        Vec4(from)
    }
}

impl<T> From<Vec4<T>> for [T; 4] {
    fn from(from: Vec4<T>) -> Self {
        from.0
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Mat4<T>([[T; 4]; 4]);

impl<T> AsRef<[[T; 4]; 4]> for Mat4<T> {
    fn as_ref(&self) -> &[[T; 4]; 4] {
        &self.0
    }
}

impl<T> From<Mat4<T>> for [[T; 4]; 4] {
    fn from(from: Mat4<T>) -> Self {
        from.0
    }
}

impl<T> Index<usize> for Mat4<T> {
    type Output = Vec4<T>;

    fn index(&self, idx: usize) -> &Self::Output {
        self.0[idx].as_ref()
    }
}

impl<T> IndexMut<usize> for Mat4<T> {
    fn index_mut(&mut self, idx: usize) -> &mut Vec4<T> {
        self.0[idx].as_mut()
    }
}

impl<T> Mul<Mat4<T>> for Mat4<T>
where
    T: Mul<T, Output = T> + Add<T, Output = T> + Copy,
    Vec4<T>: Mul<T, Output = Vec4<T>>,
{
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        {
            let a = self[0];
            let b = self[1];
            let c = self[2];
            let d = self[3];

            #[cfg_attr(rustfmt, rustfmt_skip)]
            Mat4([
                (a * other[0][0] + b * other[0][1] + c * other[0][2] + d * other[0][3]).into(),
                (a * other[1][0] + b * other[1][1] + c * other[1][2] + d * other[1][3]).into(),
                (a * other[2][0] + b * other[2][1] + c * other[2][2] + d * other[2][3]).into(),
                (a * other[3][0] + b * other[3][1] + c * other[3][2] + d * other[3][3]).into(),
            ])
        }
    }
}

impl Mat4<f32> {
    pub fn look_to_lh(eye: Vec3<f32>, dir: Vec3<f32>, up: Vec3<f32>) -> Option<Self> {
        let f = (-dir).normalize()?;
        let s = f.cross(up).normalize()?;
        let u = s.cross(f);

        #[cfg_attr(rustfmt, rustfmt_skip)]
        Some(Self([
            [s[0], u[0], - f[0], 0.0],
            [s[1], u[1], - f[1], 0.0],
            [s[2], u[2], - f[2], 0.0],
            [- eye.dot(s), - eye.dot(u), eye.dot(f), 1.0],
        ]))
    }

    pub fn perspective_lh(aspect: f32, fovy: f32, near: f32, far: f32) -> Self {
        let half_fovy_cot = 1.0 / (fovy / 2.0).tan();

        #[cfg_attr(rustfmt, rustfmt_skip)]
        Self([
            [half_fovy_cot / aspect, 0.0, 0.0, 0.0],
            [0.0, half_fovy_cot, 0.0, 0.0],
            [0.0, 0.0, (far + near) / (far - near), 1.0],
            [0.0, 0.0, (2.0 * far * near) / (near - far), 0.0],
        ])
    }

    pub fn identity() -> Self {
        Mat4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
}

#[derive(Serialize, Deserialize, PartialEq, Copy, Clone, Debug)]
pub struct Quat {
    vector: Vec3<f32>,
    scalar: f32,
}

impl Quat {
    pub fn from_axis_angle(axis: Vec3<f32>, angle: f32) -> Self {
        let (sin, scalar) = (angle / 2.0).sin_cos();
        Self {
            vector: axis.map(|c| c * sin),
            scalar,
        }
    }

    pub fn normalize(&self) -> Option<Self> {
        let d = (self.vector[0].powi(2)
            + self.vector[1].powi(2)
            + self.vector[2].powi(2)
            + self.scalar.powi(2))
        .sqrt();

        if d == 0.0 {
            return None;
        }

        Some(Self {
            vector: self.vector.map(|c| c / d),
            scalar: self.scalar / d,
        })
    }
}

impl Mul<Vec3<f32>> for Quat {
    type Output = Vec3<f32>;

    fn mul(self, other: Vec3<f32>) -> Self::Output {
        &self * other
    }
}

impl Mul<Vec3<f32>> for &Quat {
    type Output = Vec3<f32>;

    fn mul(self, other: Vec3<f32>) -> Self::Output {
        let tmp = self.vector.cross(other) + (other * self.scalar);
        (self.vector.cross(tmp) * 2.0) + other
    }
}

impl<Q> Mul<Q> for Quat
where
    Q: Borrow<Quat>,
{
    type Output = Quat;

    fn mul(self, other: Q) -> Self::Output {
        &self * other
    }
}

impl<Q> Mul<Q> for &Quat
where
    Q: Borrow<Quat>,
{
    type Output = Quat;

    fn mul(self, other: Q) -> Self::Output {
        let other = other.borrow();
        Quat {
            scalar: self.scalar * other.scalar
                - self.vector[0] * other.vector[0]
                - self.vector[1] * other.vector[1]
                - self.vector[2] * other.vector[2],
            vector: Vec3::new([
                self.scalar * other.vector[0]
                    + self.vector[0] * other.scalar
                    + self.vector[1] * other.vector[2]
                    - self.vector[2] * other.vector[1],
                self.scalar * other.vector[1]
                    + self.vector[1] * other.scalar
                    + self.vector[2] * other.vector[0]
                    - self.vector[0] * other.vector[2],
                self.scalar * other.vector[2]
                    + self.vector[2] * other.scalar
                    + self.vector[0] * other.vector[1]
                    - self.vector[1] * other.vector[0],
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mat4_perspective_lh_0() {
        let aspect = 16.0 / 9.0;
        let fovy = 0.5;
        let near = 1.0;
        let far = 100.0;
        let res: [[f32; 4]; 4] = Mat4::perspective_lh(aspect, fovy, near, far).into();
        let ctrl: [[f32; 4]; 4] = nalgebra_glm::perspective_lh(aspect, fovy, near, far).into();
        assert_eq!(res, ctrl);
    }

    #[test]
    fn test_mat4_look_at_lh_0() {
        let res: [[f32; 4]; 4] = Mat4::look_to_lh(
            [0.0, 0.0, 0.0].into(),
            [0.5, 0.0, 0.0].into(),
            [0.0, 0.0, 1.0].into(),
        )
        .unwrap()
        .into();

        let ctrl: [[f32; 4]; 4] = nalgebra_glm::translate(
            &nalgebra_glm::quat_to_mat4(&nalgebra_glm::quat_look_at_lh(
                &[0.5, 0.0, 0.0].into(),
                &[0.0, 0.0, 1.0].into(),
            )),
            &[0.0, 0.0, 0.0].into(),
        )
        .into();
        assert_eq!(res, ctrl);
    }

    #[test]
    fn test_quat_from_axis_angle_0() {
        let axis = [1.0, 0.0, 0.0];
        let angle = 1.5;

        let res = Quat::from_axis_angle(axis.into(), angle);

        let ctrl = nalgebra_glm::quat_angle_axis(1.5, &axis.into());

        type A = [f32; 3];

        assert_eq!(A::from(res.vector), A::from(ctrl.vector()));
        assert_eq!(res.scalar, ctrl.scalar());
    }

    #[test]
    fn test_quat_from_axis_angle_1() {
        let axis = [1.0, 0.5, 100.0];
        let angle = 43.5;

        let res = Quat::from_axis_angle(Vec3::<f32>::from(axis).normalize().unwrap(), angle);

        let ctrl = nalgebra_glm::quat_angle_axis(angle, &axis.into());

        type A = [f32; 3];

        assert_eq!(A::from(res.vector), A::from(ctrl.vector()));
        assert_eq!(res.scalar, ctrl.scalar());
    }

    #[test]
    fn test_quat_mul_vec3_0() {
        let axis = [1.0, 0.0, 0.0];
        let angle = 0.5;
        let vec3 = [1.3, 0.0, 3.3];

        let res = Quat::from_axis_angle(axis.into(), angle) * Vec3::from(vec3);

        let ctrl = nalgebra_glm::quat_rotate_vec3(
            &nalgebra_glm::quat_angle_axis(angle, &axis.into()),
            &vec3.into(),
        );

        type A = [f32; 3];

        assert_eq!(A::from(res), A::from(ctrl));
    }

    #[test]
    fn test_quat_mul_vec3_1() {
        let axis = [1.0, 0.5, 1.0];
        let angle = 83.5;
        let vec3 = [1.3, 0.0, 3.3];

        let res = Quat::from_axis_angle(Vec3::<f32>::from(axis).normalize().unwrap(), angle)
            * Vec3::from(vec3);

        let ctrl = nalgebra_glm::quat_rotate_vec3(
            &nalgebra_glm::quat_angle_axis(angle, &axis.into()),
            &vec3.into(),
        );

        type A = [f32; 3];

        assert_eq!(A::from(res), A::from(ctrl));
    }

    #[test]
    fn test_quat_mul_quat_0() {
        let axis1 = [1.0, 0.5, 1.0];
        let angle1 = 83.5;
        let axis2 = [0.0, 0.5, 1.0];
        let angle2 = -0.5;

        let res = Quat::from_axis_angle(Vec3::<f32>::from(axis1).normalize().unwrap(), angle1)
            * Quat::from_axis_angle(Vec3::<f32>::from(axis2).normalize().unwrap(), angle2);

        let ctrl = nalgebra_glm::quat_angle_axis(angle1, &axis1.into())
            * nalgebra_glm::quat_angle_axis(angle2, &axis2.into());

        type A = [f32; 3];

        assert_eq!(A::from(res.vector), A::from(ctrl.vector()));
        assert_eq!(res.scalar, ctrl.scalar());
    }
}
