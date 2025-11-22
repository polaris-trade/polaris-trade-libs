# Task Manager

A library for managing concurrent tasks with graceful shutdown capabilities.

## Overview

`task_manager` provides a simple yet powerful abstraction for running multiple async tasks with:

- Graceful shutdown coordination
- Signal handling (SIGINT/SIGTERM/Ctrl+C)
- Automatic error propagation
- Configurable shutdown timeouts
- Clean error chains for debugging

## Error Handling Philosophy

This library follows **modular error handling** principles:

- ✅ **Library errors are specific**: Each operation that can fail has its own error type
- ✅ **Errors chain properly**: Using `Error::source()` for good backtraces
- ✅ **Extensible design**: `#[non_exhaustive]` allows adding fields without breaking changes
- ✅ **No `anyhow` dependency**: Libraries define their own error types

See [Error Handling Guidelines](../../../docs/ERROR_HANDLING_GUIDELINES.md) for the full rationale.

## Usage in Applications with `anyhow`

### Basic Usage

Applications typically use `anyhow::Result<T>` for convenience. The task_manager errors integrate seamlessly:

```rust
use task_manager::{TaskManager, TaskManagerConfig, RunnableTask};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let mut manager = TaskManager::with_defaults();

    // Register your tasks
    manager.register(MyTask::new());
    manager.register(AnotherTask::new());

    // Run returns ShutdownResult<()> which auto-converts to anyhow::Result
    manager.run().await?;

    Ok(())
}
```

**The `?` operator automatically converts `ShutdownError` to `anyhow::Error`!**

### Error Display with Context

When a shutdown error occurs, `anyhow` will display the full error chain:

```
Error: shutdown failed (timeout: 30s): operation timed out
```

Or for subsystem failures:

```
Error: shutdown failed: 2 subsystem(s) failed
  1: task 'database_worker' failed: execution error
  2: task 'api_server' failed: execution error
```

### Adding Application Context

Use `anyhow::Context` to add application-specific context:

```rust
use anyhow::Context;

#[tokio::main]
async fn main() -> Result<()> {
    let mut manager = TaskManager::new(TaskManagerConfig {
        shutdown_timeout: Duration::from_secs(60),
        catch_signals: true,
        shutdown_on_error: true,
    });

    manager.register(DatabaseWorker::new());
    manager.register(ApiServer::new());

    manager.run().await
        .context("Failed to run application tasks")?;

    Ok(())
}
```

This produces:

```
Error: Failed to run application tasks

Caused by:
    shutdown failed (timeout: 60s): operation timed out
```

### Handling Specific Errors

If you need to handle specific error types, match before converting to `anyhow`:

```rust
use task_manager::{ShutdownError, ShutdownErrorKind};

match manager.run().await {
    Ok(()) => println!("Clean shutdown"),
    Err(e) => {
        match &e.kind {
            ShutdownErrorKind::Timeout => {
                eprintln!("Shutdown timed out - forcing exit");
                std::process::exit(1);
            }
            ShutdownErrorKind::SubsystemsFailed { failures } => {
                for failure in failures {
                    eprintln!("Task '{}' failed: {}", failure.task_name, failure.kind);
                }
                return Err(e.into());
            }
        }
    }
}
```

### Implementing RunnableTask

Tasks return `TaskResult<()>` which is `Result<(), TaskError>`:

```rust
use async_trait::async_trait;
use task_manager::{RunnableTask, TaskResult, TaskError, TaskErrorKind};
use tokio_graceful_shutdown::SubsystemHandle;

pub struct MyWorker {
    config: WorkerConfig,
}

#[async_trait]
impl RunnableTask for MyWorker {
    fn name(&self) -> &str {
        "my_worker"
    }

    async fn run(&self, subsys: SubsystemHandle) -> TaskResult<()> {
        loop {
            tokio::select! {
                _ = subsys.on_shutdown_requested() => {
                    println!("Shutdown requested");
                    break;
                }
                result = self.process_work() => {
                    // Convert your errors to TaskError
                    result.map_err(|e| {
                        TaskError::new(
                            self.name(),
                            TaskErrorKind::Execution {
                                source: e.into()
                            }
                        )
                    })?;
                }
            }
        }

        Ok(())
    }

    async fn on_shutdown(&self) -> TaskResult<()> {
        // Optional cleanup
        println!("Cleaning up resources");
        Ok(())
    }
}
```

### Converting Application Errors to TaskError

For application errors, create helper conversions:

```rust
use std::error::Error as StdError;

// Helper to convert any error into TaskError
fn to_task_error<E>(task_name: &str, error: E) -> TaskError
where
    E: Into<Box<dyn StdError + Send + Sync>>
{
    TaskError::new(
        task_name,
        TaskErrorKind::Execution {
            source: error.into()
        }
    )
}

// Usage in your task:
async fn run(&self, subsys: SubsystemHandle) -> TaskResult<()> {
    self.do_work()
        .await
        .map_err(|e| to_task_error(self.name(), e))?;
    Ok(())
}
```

### Using anyhow Errors in Tasks (Anti-pattern, but possible)

If your task already uses `anyhow::Error` internally, you can convert it:

```rust
async fn run(&self, subsys: SubsystemHandle) -> TaskResult<()> {
    // Your function returns anyhow::Result<()>
    self.complex_operation()
        .await
        .map_err(|e| TaskError::execution(self.name(), format!("{:#}", e)))?;

    Ok(())
}
```

**However**, it's better to use specific error types in your task implementation and only use `anyhow` at the application boundary (main function).

## Error Type Reference

### `TaskError`

Represents an error from a specific task.

**Fields:**

- `task_name: String` - Name of the task that failed
- `kind: TaskErrorKind` - The specific error kind

**Methods:**

- `TaskError::execution(name, source)` - Create an execution error
- `TaskError::shutdown(name, source)` - Create a shutdown error
- `TaskError::panic(name, message)` - Create a panic error

### `TaskErrorKind`

**Variants:**

- `Execution { source }` - Task execution failed
- `Shutdown { source }` - Shutdown handler failed
- `Panic { message }` - Task panicked
- `StartupFailed { message }` - Task failed to start

### `ShutdownError`

Represents an error during TaskManager shutdown.

**Fields:**

- `timeout: Option<Duration>` - Shutdown timeout, if applicable
- `kind: ShutdownErrorKind` - The specific error kind

**Methods:**

- `ShutdownError::timeout(duration)` - Create a timeout error
- `ShutdownError::subsystems_failed(failures)` - Create a subsystems failed error

### `ShutdownErrorKind`

**Variants:**

- `Timeout` - Shutdown timed out
- `SubsystemsFailed { failures: Vec<TaskError> }` - One or more tasks failed

## Best Practices

### ✅ DO

- Use `task_manager` errors in library code
- Use `anyhow` at the application boundary (main function)
- Add context with `.context()` when propagating errors
- Match on specific error kinds when you need special handling
- Implement `RunnableTask` with proper error handling

### ❌ DON'T

- Use `anyhow::Error` in the `RunnableTask::run()` signature
- Lose error context by converting to strings too early
- Ignore shutdown errors without logging
- Use `unwrap()` or `expect()` in production task code

## Examples

See the `examples/` directory for complete examples:

- `basic.rs` - Simple task manager usage
- `with_context.rs` - Adding application context
- `error_handling.rs` - Handling specific error types
- `graceful_shutdown.rs` - Signal handling and clean shutdown

## License

See the workspace LICENSE file.
