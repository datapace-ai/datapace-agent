# Contributing to Datapace Agent

Thank you for your interest in contributing to Datapace Agent! This document provides guidelines and instructions for contributing.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for everyone.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Docker (for integration tests)
- PostgreSQL (for local testing)

### Development Setup

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/YOUR_USERNAME/datapace-agent.git
   cd datapace-agent
   ```

2. Install development dependencies:
   ```bash
   cargo install cargo-watch cargo-audit
   ```

3. Start a local PostgreSQL for testing:
   ```bash
   docker-compose --profile dev up -d postgres
   ```

4. Run the agent in development mode:
   ```bash
   export DATABASE_URL=postgres://datapace:datapace@localhost:5432/datapace_dev
   export DATAPACE_API_KEY=test_key
   cargo watch -x run
   ```

## Development Workflow

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run integration tests (requires Docker)
cargo test --features integration -- --ignored
```

### Code Quality

Before submitting a PR, ensure your code passes all checks:

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Security audit
cargo audit

# All checks
make check
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build Docker image
docker build -t datapace-agent .
```

## Submitting Changes

### Pull Request Process

1. Create a new branch for your changes:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes, following the coding standards below

3. Write or update tests for your changes

4. Ensure all tests pass and code quality checks succeed

5. Commit your changes with a clear commit message:
   ```bash
   git commit -m "feat: add support for MySQL collector"
   ```

6. Push to your fork and create a Pull Request

### Commit Message Format

We follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `refactor:` - Code refactoring
- `test:` - Adding or updating tests
- `chore:` - Maintenance tasks

Examples:
```
feat: add MySQL collector support
fix: handle connection timeout gracefully
docs: update README with new configuration options
refactor: extract common query logic into trait
```

## Coding Standards

### Rust Style

- Follow the official [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting (run `cargo fmt`)
- Address all `clippy` warnings
- Write documentation for public APIs

### Error Handling

- Use `thiserror` for error types
- Provide context with `anyhow::Context`
- Log errors appropriately with `tracing`

### Testing

- Write unit tests for new functionality
- Use `mockall` for mocking dependencies
- Integration tests go in `tests/` directory

### Documentation

- Document all public functions and types
- Include examples in documentation where helpful
- Update README.md for user-facing changes

## Project Structure

```
datapace-agent/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Library root
│   ├── config/          # Configuration management
│   ├── collector/       # Database collectors
│   │   └── postgres/    # PostgreSQL implementation
│   ├── payload/         # Data structures for API
│   ├── uploader/        # Cloud API client
│   └── scheduler/       # Collection scheduler
├── configs/             # Example configurations
├── docs/                # Documentation
└── tests/               # Integration tests
```

## Adding a New Database Collector

1. Create a new module under `src/collector/`:
   ```
   src/collector/mysql/
   ├── mod.rs
   ├── queries.rs
   └── providers.rs
   ```

2. Implement the `Collector` trait

3. Add provider detection logic

4. Update `create_collector()` factory function

5. Add configuration options

6. Write tests

7. Update documentation

## Questions?

- Open an issue for bugs or feature requests
- Join our [Discord community](https://discord.gg/datapace) for discussions
- Email us at hello@datapace.ai

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
