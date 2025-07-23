<p align="center"><img src="docs/images/logo.png" height="250px" width="250px" alt="rinha logo"></p>
<h1 align="center">Rinha de Backend 2025 - Rust Submission</h1>

<div align="center">
  <!-- Rust version -->
  <a href="https://releases.rs/docs/1.88.0">
    <img src="https://img.shields.io/badge/rust-v1.88-purple" alt="rust version" />
  </a>
  <!-- Sonarcloud -->
  <a href="https://sonarcloud.io/summary/new_code?id=josimar-silva_rinha-de-backend-2025">
    <img src="https://sonarcloud.io/api/project_badges/measure?project=josimar-silva_rinha-de-backend-2025&metric=alert_status&token=2c8a7fe058fee6ae54b2366cbf8224ec52e4e5ea" alt="sonarcloud" />
  </a>
  <!-- Coverage -->
  <a href="https://sonarcloud.io/summary/new_code?id=josimar-silva_rinha-de-backend-2025">
    <img src="https://sonarcloud.io/api/project_badges/measure?project=josimar-silva_rinha-de-backend-2025&metric=coverage&token=2c8a7fe058fee6ae54b2366cbf8224ec52e4e5ea" alt="coverage" />
  </a>
  <!-- Docker Builds -->
  <a href="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/docker.yaml">
    <img src="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/docker.yaml/badge.svg" alt="docker builds" />
  </a>
  <!-- ci -->
  <a href="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/ci.yaml">
    <img src="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/ci.yaml/badge.svg" alt="ci" />
  </a>
  <!-- cd -->
  <a href="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/cd.yaml">
    <img src="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/cd.yaml/badge.svg" alt="cd" />
  </a>
  <!-- performance -->
  <a href="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/perf-tests.yaml">
    <img src="https://github.com/josimar-silva/rinha-de-backend-2025/actions/workflows/perf-tests.yaml/badge.svg" alt="perf" />
  </a>
</div>

<div align="center">
</div>

<div align="center">
  <strong>Submission for the <a href="https://github.com/zanfranceschi/rinha-de-backend-2025">Rinha de Backend 2025</a> challenge, implemented in Rust.</strong>
</div>

## The Challenge

The goal is to create a backend service that acts as a payment intermediary for two external payment processors. 
These processors have different fees and are subject to instability. 
The backend must implement a strategy to maximize profit by choosing the best processor for each transaction, while also providing a consistent summary of operations.

For full details, see the official [Rinha de Backend 2025 repository](https://github.com/zanfranceschi/rinha-de-backend-2025).

## This Implementation

This project is a high-performance, resource-efficient solution built entirely in Rust. 
It is designed to be robust and handle the instabilities of the payment processors gracefully.

### Stack

*   **Language:** Rust
*   **Frameworks:** Actix Web, Tokio
*   **Database/Queue:** Redis
*   **Load Balancer:** Nginx

### Design

WIP

## Running the Project

The application is fully containerized and can be run using Docker Compose.

### Prerequisites

*   Docker
*   Docker Compose

### Steps

1.  **Start the Payment Processors:**

    First, you need to start the external payment processor services. The official challenge repository provides the necessary `docker-compose.yml` for this.

    ```bash
    # In the directory of the official rinha-de-backend-2025 repository
    docker-compose -f payment-processor/docker-compose.yml up -d
    ```

2.  **Start this Application:**

    Once the payment processors are running, you can start this application. The `docker-compose.yml` in this repository is configured to connect to the payment processor network.

    ```bash
    # In the root of this repository
    docker-compose up -d
    ```

3.  **Access the Endpoints:**

    The load balancer will expose the application on port `9999`.

    *   **Process a Payment:** `POST http://localhost:9999/payments`
    *   **Get Payment Summary:** `GET http://localhost:9999/payments-summary`

## Build from Source

If you prefer to build and run the application from source without Docker, you can do so with the following commands:

```bash
# Build the project in release mode
cargo build --release

# Run the application
cargo run --release
```

## Testing

To run the integration tests for this project, use the following command:

```bash
cargo test
```

## Want to contribute?

Check the [contributing](CONTRIBUTING.md) guidelines.

## Releasing

For details on how to release a new version of this project, see the [releasing](RELEASING.md) guidelines.

