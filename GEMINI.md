# Project Overview

This project is a Rust-based submission for the Rinha de Backend 2025 challenge. It acts as a payment intermediary, designed to be high-performance and resilient.

## Architectural Style

The project adheres to the principles of **Clean Architecture**, promoting a clear separation of concerns and testability. Key layers include:

-   **Domain Layer (`src/domain/`):** Contains the core business logic, entities, and use cases, independent of external frameworks or databases.
-   **Infrastructure Layer (`src/infrastructure/`):** Implements interfaces defined in the domain layer, handling external concerns like database interactions (Redis), queueing, and external service integrations.
-   **Adapters Layer (`src/adapters/`):** Provides the entry points for external actors, such as web API handlers (`src/adapters/web/`), converting external requests into domain-understandable formats and vice-versa.

This layered approach ensures that changes in external technologies do not impact the core business rules, making the system more robust, maintainable, and scalable.

## Key Technologies

- **Language:** Rust
- **Framework:** Actix Web
- **Async Runtime:** Tokio
- **Database/Queue:** Redis
- **Load Balancer:** Nginx

## Development Practices

- **Clean Code:** The project follows clean code principles to ensure readability and maintainability.
- **TDD (Test-Driven Development):** The project is developed using TDD practices, with a strong emphasis on testing.

## Development Workflow

- **Atomic Commits:** Changes should be made in smaller, atomic commits.
- **Post-Change Checks:** After any code modification, the following should be run:
  - `just lint`: Lint the code.
  - `just format`: Format the code.
  - `just test`: Run all tests.

## Project Structure

- `src/`: Contains the main source code.
  - `api/`: Handlers for API endpoints and error handling.
  - `model/`: Data structures and business logic.
  - `workers/`: Background workers for tasks like health checks and payment processing.
- `tests/`: Integration tests.
- `payment-processor/`: Docker-compose for external payment processors.
- `Dockerfile`: For containerizing the application.
- `docker-compose.yml`: For running the application and its dependencies.
- `justfile`: Defines custom commands for development.

## Commands

- `just test`: Run tests with code coverage.
- `just format`: Format the code.
- `just lint`: Lint the code.
- `just clean-containers`: Remove all Docker containers.
- `cargo build --release`: Build the project for release.
- `cargo run --release`: Run the application from source.
- `cargo test`: Run integration tests.
- `docker-compose up -d`: Start the application and its dependencies in detached mode.
