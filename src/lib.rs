#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![doc = include_str!("../README.md")]

mod content;
mod decode;
mod encode;
pub mod extract;
pub mod handler;
mod macros;
mod rejection;
pub mod routing;

pub use {
	content::ContentType, decode::CodecDecode, encode::CodecEncode, extract::Accept,
	handler::CodecHandler, rejection::CodecRejection,
};

use axum::{
	body::Bytes,
	extract::{FromRequest, Request},
	http::header,
	response::{IntoResponse, Response},
};

#[doc(hidden)]
pub mod __private {
	#[cfg(feature = "bincode")]
	pub use bincode;
	#[cfg(feature = "bitcode")]
	pub use bitcode;
	#[cfg(feature = "aide")]
	pub use schemars;
	#[cfg(feature = "serde")]
	pub use serde;
	#[cfg(feature = "validator")]
	pub use validator;
}

#[cfg(feature = "macros")]
pub use axum_codec_macros::apply;

/// Codec extractor / response.
///
/// The serialized data is not specified, unlike [`axum::Json`]. Upon deserialization, the request's
/// `Content-Type` header is used to determine the format of the data.
///
/// The supported formats are:
/// - `JSON`
/// - `MessagePack`
/// - `Bincode`
/// - `Bitcode`
/// - `YAML`
/// - `TOML`
///
/// By default, only JSON is supported. To enable other formats, use the corresponding feature flags.
///
/// Note that [`IntoResponse`] is not implemented for this type, as the headers are not available
/// when serializing the data. Instead, use [`Codec::to_response`] to create a response with the
/// appropriate `Content-Type` header extracted from the request with [`Accept`].
///
/// # Examples
///
/// ```edition2021
/// # use axum_codec::{Codec, ContentType};
/// # use axum::http::HeaderValue;
/// # use serde_json::json;
/// #
/// # fn main() {
/// #[axum_codec::apply(decode)]
/// struct Greeting {
///   hello: String
/// }
///
/// let bytes = b"{\"hello\": \"world\"}";
/// let content_type = ContentType::Json;
///
/// let Codec(data) = Codec::<Greeting>::from_bytes(bytes, content_type).unwrap();
///
/// assert_eq!(data.hello, "world");
/// # }
/// ```
pub struct Codec<T>(pub T);

#[cfg(feature = "aide")]
impl<T> aide::operation::OperationInput for Codec<T>
where
	T: schemars::JsonSchema,
{
	fn operation_input(ctx: &mut aide::gen::GenContext, operation: &mut aide::openapi::Operation) {
		axum::Json::<T>::operation_input(ctx, operation);
	}

	fn inferred_early_responses(
		ctx: &mut aide::gen::GenContext,
		operation: &mut aide::openapi::Operation,
	) -> Vec<(Option<u16>, aide::openapi::Response)> {
		axum::Json::<T>::inferred_early_responses(ctx, operation)
	}
}

#[cfg(feature = "aide")]
impl<T> aide::operation::OperationOutput for Codec<T>
where
	T: schemars::JsonSchema,
{
	type Inner = <axum::Json<T> as aide::operation::OperationOutput>::Inner;

	fn operation_response(
		ctx: &mut aide::gen::GenContext,
		operation: &mut aide::openapi::Operation,
	) -> Option<aide::openapi::Response> {
		axum::Json::<T>::operation_response(ctx, operation)
	}

	fn inferred_responses(
		ctx: &mut aide::gen::GenContext,
		operation: &mut aide::openapi::Operation,
	) -> Vec<(Option<u16>, aide::openapi::Response)> {
		axum::Json::<T>::inferred_responses(ctx, operation)
	}
}

#[cfg(feature = "validator")]
impl<T> validator::Validate for Codec<T>
where
	T: validator::Validate,
{
	fn validate(&self) -> Result<(), validator::ValidationErrors> {
		self.0.validate()
	}
}

impl<T> Codec<T>
where
	T: CodecEncode,
{
	pub fn to_response<C: Into<ContentType>>(&self, content_type: C) -> Response {
		let content_type = content_type.into();
		let bytes = match self.to_bytes(content_type) {
			Ok(bytes) => bytes,
			Err(rejection) => return rejection.into_response(),
		};

		([(header::CONTENT_TYPE, content_type.into_header())], bytes).into_response()
	}
}

#[axum::async_trait]
impl<T, S> FromRequest<S> for Codec<T>
where
	T: CodecDecode,
	S: Send + Sync + 'static,
{
	type Rejection = CodecRejection;

	async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
		let content_type = req
			.headers()
			.get(header::CONTENT_TYPE)
			.and_then(ContentType::from_header)
			.unwrap_or_default();

		let bytes = Bytes::from_request(req, state).await?;
		let data = Codec::from_bytes(&bytes, content_type)?;

		Ok(data)
	}
}

/// Defines the [`CodecDecode`] and [`CodecEncode`] traits with the given constraints.
macro_rules! codec_trait {
	($id:ident, $($constraint:tt)*) => {
		pub trait $id: $($constraint)* {}
		impl<T> $id for T where T: $($constraint)* {}
	};
	($id:ident) => {
		pub trait $id {}
		impl<T> $id for T {}
	};
}

pub(crate) use codec_trait;

#[cfg(test)]
mod test {
	use super::{Codec, ContentType};
	use crate as axum_codec;

	#[super::apply(decode)]
	#[derive(Debug, PartialEq, Eq)]
	struct Data {
		hello: String,
	}

	#[test]
	fn test_json_codec() {
		let bytes = b"{\"hello\": \"world\"}";

		let Codec(data) = Codec::<Data>::from_bytes(bytes, ContentType::Json).unwrap();

		assert_eq!(
			data,
			Data {
				hello: "world".into()
			}
		);
	}

	#[test]
	fn test_msgpack_codec() {
		let bytes = b"\x81\xa5hello\xa5world";

		let Codec(data) = Codec::<Data>::from_bytes(bytes, ContentType::MsgPack).unwrap();

		assert_eq!(
			data,
			Data {
				hello: "world".into()
			}
		);
	}
}
