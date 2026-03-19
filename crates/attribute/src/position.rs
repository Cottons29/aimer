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





