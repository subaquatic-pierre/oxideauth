use crate::core::error::CoreResult;

pub trait ValidateParams
where
    Self: Sized,
{
    fn validate(self) -> CoreResult<Self>;
}
