use std::sync::Arc;
use std::time::Duration;

use rinha_de_backend::domain::payment::Payment;
use rinha_de_backend::domain::queue::{Message, Queue};
use rinha_de_backend::infrastructure::config::redis::PAYMENTS_QUEUE_KEY;
use rinha_de_backend::infrastructure::queue::redis_payment_queue::PaymentQueue;
use tokio::sync::mpsc;
use uuid::Uuid;

mod support;

use crate::support::redis_container::get_test_redis_client;

#[tokio::test]
async fn test_payment_queue_push_and_pop() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client;
	let payment_queue = PaymentQueue::new(redis_client.clone());

	let payment = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         10000.28,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let message = Message::with(Uuid::new_v4(), payment.clone());

	payment_queue.push(message.clone()).await.unwrap();

	let popped_message = payment_queue.pop().await.unwrap().unwrap();

	assert_eq!(popped_message.id, message.id);
	assert_eq!(popped_message.body.correlation_id, payment.correlation_id);
	assert_eq!(popped_message.body.amount, payment.amount);
}

#[tokio::test]
async fn test_payment_queue_pop_empty() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client;
	let payment_queue = PaymentQueue::new(redis_client.clone());

	let popped_message = payment_queue.pop().await.unwrap();

	assert!(popped_message.is_none());
}

#[tokio::test]
async fn test_payment_queue_multiple_pushes_and_pops() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client;
	let payment_queue = PaymentQueue::new(redis_client.clone());

	let payment1 = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         10000.34,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};
	let payment2 = Payment {
		correlation_id: Uuid::new_v4(),
		amount:         20000.28,
		requested_at:   None,
		processed_at:   None,
		processed_by:   None,
	};

	let message1 = Message::with(Uuid::new_v4(), payment1.clone());
	let message2 = Message::with(Uuid::new_v4(), payment2.clone());

	payment_queue.push(message1.clone()).await.unwrap();
	payment_queue.push(message2.clone()).await.unwrap();

	let popped_message1 = payment_queue.pop().await.unwrap().unwrap();
	let popped_message2 = payment_queue.pop().await.unwrap().unwrap();

	assert_eq!(popped_message1.id, message1.id);
	assert_eq!(popped_message1.body.correlation_id, payment1.correlation_id);
	assert_eq!(popped_message1.body.amount, payment1.amount);

	assert_eq!(popped_message2.id, message2.id);
	assert_eq!(popped_message2.body.correlation_id, payment2.correlation_id);
	assert_eq!(popped_message2.body.amount, payment2.amount);
}

#[tokio::test]
async fn test_payment_queue_fault_tolerance() {
	let redis_container = get_test_redis_client().await;
	let redis_client = redis_container.client.clone();
	let payment_queue = PaymentQueue::new(redis_client.clone());

	let mut conn = redis_client
		.get_multiplexed_async_connection()
		.await
		.unwrap();

	redis::cmd("LPUSH")
		.arg(PAYMENTS_QUEUE_KEY)
		.arg("this is not a valid message")
		.query_async::<()>(&mut conn)
		.await
		.unwrap();

	let popped_message = payment_queue.pop().await;
	assert!(popped_message.is_err());
}

#[tokio::test]
async fn test_payment_queue_under_load() {
	// Arrange
	let redis = get_test_redis_client().await;
	let payment_queue = Arc::new(PaymentQueue::new(redis.client.clone()));

	const NUM_PAYMENTS: usize = 10000;
	const NUM_WORKERS: usize = 4;

	let (tx, mut rx) = mpsc::channel(NUM_PAYMENTS);

	// Push payments to the queue
	for i in 0..NUM_PAYMENTS {
		let payment = Payment {
			correlation_id: Uuid::new_v4(),
			amount:         (i + 1) as f64,
			requested_at:   None,
			processed_at:   None,
			processed_by:   None,
		};
		payment_queue
			.push(Message::with(Uuid::new_v4(), payment))
			.await
			.unwrap();
	}

	// Act
	// Spawn workers to process payments
	for _ in 0..NUM_WORKERS {
		let queue = Arc::clone(&payment_queue);
		let tx = tx.clone();
		tokio::spawn(async move {
			while let Ok(Some(message)) = queue.pop().await {
				if tx.send(message).await.is_err() {
					break;
				}
			}
		});
	}

	// Assert
	// Collect results
	let mut processed_payments = Vec::new();
	for _ in 0..NUM_PAYMENTS {
		match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
			Ok(Some(payment)) => processed_payments.push(payment),
			_ => break,
		}
	}

	assert_eq!(
		processed_payments.len(),
		NUM_PAYMENTS,
		"All payments should be processed"
	);
}
