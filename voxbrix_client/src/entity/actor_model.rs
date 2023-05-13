pub trait MinMax {
    const MIN: Self;
    const MAX: Self;
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorModel(pub usize);

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorAnimation(pub usize);

impl MinMax for ActorAnimation {
    const MAX: Self = Self(usize::MAX);
    const MIN: Self = Self(usize::MIN);
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorBodyPart(pub usize);

impl MinMax for ActorBodyPart {
    const MAX: Self = Self(usize::MAX);
    const MIN: Self = Self(usize::MIN);
}