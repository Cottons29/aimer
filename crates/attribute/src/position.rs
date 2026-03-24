use std::ops::{Add, Mul, Sub, SubAssign};
use crate::Float;
use crate::size::ResolvedSize;

macro_rules! impl_from_num {
    ($t:ty) => {
        impl From<$t> for Vec2d {
            fn from(x: $t) -> Self {
                Self { x: x as Float, y: x as Float }
            }
        }
    };
}

macro_rules! impl_from_tuple {
    ($t:ty) => {
        impl From<$t> for Vec2d {
            fn from((x, y): $t) -> Self {
                Self { x: x as Float, y: y as Float }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec2d {
    pub x: Float,
    pub y: Float,
}

impl Mul<Vec2d> for Float {
    type Output = Vec2d;
    fn mul(self, rhs: Vec2d) -> Self::Output {
        Self::Output { x: self * rhs.x, y: self * rhs.y }
    }
}

impl Mul<Float> for Vec2d {
    type Output = Vec2d;
    fn mul(self, rhs: Float) -> Self::Output {
        Self::Output { x: self.x * rhs, y: self.y * rhs }
    }
}

impl Add<Vec2d> for Vec2d {
    type Output = Vec2d;
    fn add(self, rhs: Vec2d) -> Self::Output {
        Self::Output { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Add<Float> for Vec2d {
    type Output = Vec2d;
    fn add(self, rhs: Float) -> Self::Output {
        Self::Output { x: self.x + rhs, y: self.y + rhs }
    }
}

impl Sub<(Float, Float)> for Vec2d {
    type Output = Vec2d;
    fn sub(self, rhs: (Float, Float)) -> Self::Output {
        Self::Output { x: self.x - rhs.0, y: self.y - rhs.1 }
    }
}

impl SubAssign<(Float, Float)> for Vec2d {
    fn sub_assign(&mut self, rhs: (Float, Float)) {
        self.x -= rhs.0;
        self.y -= rhs.1;
    }
}

impl Vec2d {
    pub fn get_end(&self, size: ResolvedSize) -> Vec2d {
        Self {
            x: self.x + size.width,
            y: self.y + size.height,
        }
    }
}

impl From<(Float, Float)> for Vec2d {
    fn from((x, y): (Float, Float)) -> Self {
        Self { x, y }
    }
}

impl From<Float> for Vec2d {
    fn from(x: Float) -> Self {
        Self { x, y: x }
    }
}

impl_from_num!(usize);
impl_from_num!(i8);
impl_from_num!(u8);
impl_from_num!(i16);
impl_from_num!(u16);
impl_from_num!(u32);
impl_from_num!(i32);
impl_from_num!(i64);
impl_from_num!(u64);

impl_from_tuple!((usize, usize));
impl_from_tuple!((i8, i8));
impl_from_tuple!((u8, u8));
impl_from_tuple!((i16, i16));
impl_from_tuple!((u16, u16));
impl_from_tuple!((u32, u32));
impl_from_tuple!((i32, i32));
impl_from_tuple!((i64, i64));
impl_from_tuple!((u64, u64));





