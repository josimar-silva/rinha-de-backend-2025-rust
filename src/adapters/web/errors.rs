use actix_web::http::StatusCode;
use actix_web::http::header::ContentType;
use actix_web::{HttpResponse, error};
use derive_more::derive::{Display, Error};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorResponse {
	#[serde(rename = "statusCode")]
	status_code: u16,
	error:       String,
	message:     String,
}

#[derive(Debug, Display, Error)]
pub enum ApiError {
	#[display("Could not connect to the database.")]
	DatabaseConnectionError,
	#[display("Could not perform this operation.")]
	TransactionError,
	#[display("Request data is invalid.")]
	BadClientDataError,
	#[display("Internal server error.")]
	InternalServerError,
}

impl ApiError {
	pub fn name(&self) -> String {
		match self {
			ApiError::DatabaseConnectionError => "Insufficient Storage".to_string(),
			ApiError::TransactionError => "Unprocessable Entity".to_string(),
			ApiError::BadClientDataError => "Bad request".to_string(),
			ApiError::InternalServerError => "Internal Server Error".to_string(),
		}
	}
}

impl error::ResponseError for ApiError {
	fn error_response(&self) -> HttpResponse {
		HttpResponse::build(self.status_code())
			.content_type(ContentType::json())
			.json(ErrorResponse {
				status_code: self.status_code().as_u16(),
				error:       self.to_string(),
				message:     self.name(),
			})
	}

	fn status_code(&self) -> StatusCode {
		match self {
			ApiError::DatabaseConnectionError => StatusCode::INSUFFICIENT_STORAGE,
			ApiError::TransactionError => StatusCode::UNPROCESSABLE_ENTITY,
			ApiError::BadClientDataError => StatusCode::BAD_REQUEST,
			ApiError::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
		}
	}
}

impl From<Box<dyn std::error::Error>> for ApiError {
	fn from(_: Box<dyn std::error::Error>) -> Self {
		ApiError::InternalServerError
	}
}

#[cfg(test)]
mod tests {
	use actix_web::error::ResponseError;

	use super::*;

	#[test]
	fn test_database_connection_error() {
		let error = ApiError::DatabaseConnectionError;
		assert_eq!(error.name(), "Insufficient Storage");
		assert_eq!(error.status_code(), StatusCode::INSUFFICIENT_STORAGE);

		let resp = error.error_response();
		assert_eq!(resp.status(), StatusCode::INSUFFICIENT_STORAGE);
	}

	#[test]
	fn test_transaction_error() {
		let error = ApiError::TransactionError;
		assert_eq!(error.name(), "Unprocessable Entity");
		assert_eq!(error.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

		let resp = error.error_response();
		assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
	}

	#[test]
	fn test_bad_client_data_error() {
		let error = ApiError::BadClientDataError;
		assert_eq!(error.name(), "Bad request");
		assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);

		let resp = error.error_response();
		assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
	}
}
