/// Non-static alternative to [`std::any::TypeId`]
///
/// It erases all lifetime data, beware of that.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub struct NsTypeId(usize);

impl NsTypeId {
    pub fn of<T>() -> Self {
        Self(Self::of::<T> as usize)
    }
}
