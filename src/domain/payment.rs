use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Payment {
	#[serde(rename = "correlationId")]
	pub correlation_id: Uuid,
	pub amount:         f64,
	#[serde(
		rename = "requestedAt",
		with = "time::serde::rfc3339::option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub requested_at:   Option<OffsetDateTime>,
	#[serde(
		rename = "processedAt",
		with = "time::serde::rfc3339::option",
		skip_serializing_if = "Option::is_none",
		default
	)]
	pub processed_at:   Option<OffsetDateTime>,
	#[serde(skip_serializing_if = "Option::is_none", default)]
	pub processed_by:   Option<String>,
}

#[cfg(test)]
mod tests {
	use rinha_de_backend::domain::payment::Payment;
	use serde_json;
	use time::OffsetDateTime;
	use uuid::Uuid;

	#[test]
	fn test_payment_serialization() {
		let correlation_id =
			Uuid::parse_str("7b3739e4-5be8-4f98-84a7-a13fd5984059").unwrap();
		let requested_at = OffsetDateTime::parse(
			"2017-07-21T17:32:28Z",
			&time::format_description::well_known::Rfc3339,
		)
		.unwrap();

		let payment = Payment {
			correlation_id,
			amount: 1.0,
			requested_at: Some(requested_at),
			processed_at: None,
			processed_by: None,
		};

		let expected_json = serde_json::json!({
			"correlationId": "7b3739e4-5be8-4f98-84a7-a13fd5984059",
			"amount": 1.0,
			"requestedAt": "2017-07-21T17:32:28Z"
		});

		let serialized_payment = serde_json::to_value(&payment).unwrap();

		assert_eq!(serialized_payment, expected_json);
	}
}
