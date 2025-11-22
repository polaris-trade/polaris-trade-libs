pub use crate::error::{ShutdownError, ShutdownResult, TaskError, TaskErrorKind, TaskResult};
use crate::{
    RunnableTask,
    core_allocator::{CoreAffinityConfig, CoreAllocator},
};
use logger::{error, info, warn};
use std::{fmt::Debug, sync::Arc, time::Duration};
use tokio_graceful_shutdown::{
    SubsystemBuilder, SubsystemHandle, Toplevel, errors::GracefulShutdownError,
};
pub use tokio_util::sync::CancellationToken;

/// Configuration for TaskManager.
#[derive(Debug, Clone, Copy)]
pub struct TaskManagerConfig {
    /// Timeout for graceful shutdown.
    pub shutdown_timeout: Duration,

    /// Whether to catch OS signals (SIGINT/SIGTERM/Ctrl+C).
    pub catch_signals: bool,

    /// Whether subsystem failures should trigger global shutdown.
    pub shutdown_on_error: bool,

    /// Whether to validate core allocation configurations.
    pub validate_core_allocation: bool,
}

impl Default for TaskManagerConfig {
    fn default() -> Self {
        Self {
            shutdown_timeout: Duration::from_secs(30),
            catch_signals: true,
            shutdown_on_error: true,
            validate_core_allocation: true,
        }
    }
}

pub struct TaskManager {
    tasks: Vec<Arc<dyn RunnableTask>>,
    task_affinities: Vec<CoreAffinityConfig>,
    config: TaskManagerConfig,
    factories: Vec<TaskFactory>,
}

impl TaskManager {
    pub fn new(config: TaskManagerConfig) -> Self {
        Self {
            tasks: Vec::new(),
            task_affinities: Vec::new(),
            config,
            factories: Vec::new(),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(TaskManagerConfig::default())
    }

    /// Register any task that implements [`RunnableTask`].
    pub fn register<T: RunnableTask>(&mut self, task: T) {
        self.tasks.push(Arc::new(task));
        self.task_affinities.push(CoreAffinityConfig::None);
    }

    /// Register any task that implements [`RunnableTask`] and pinned to specific core.
    pub fn register_with_affinity<T: RunnableTask>(
        &mut self,
        task: T,
        affinity: CoreAffinityConfig,
    ) {
        self.tasks.push(Arc::new(task));
        self.task_affinities.push(affinity);
    }

    /// Register a factory for creating multiple instances of a task.
    pub fn register_factory<F>(&mut self, name: impl Into<String>, factory: F, instances: usize)
    where
        F: Fn() -> Arc<dyn RunnableTask> + Send + Sync + 'static,
    {
        self.factories.push(TaskFactory {
            name: name.into(),
            factory: Arc::new(factory),
            instances,
            affinity: CoreAffinityConfig::None,
        });
    }

    /// Register a factory for creating multiple instances of a task and pinned to specific core.
    pub fn register_factory_with_affinity<F>(
        &mut self,
        name: impl Into<String>,
        factory: F,
        instances: usize,
        affinity: CoreAffinityConfig,
    ) where
        F: Fn() -> Arc<dyn RunnableTask> + Send + Sync + 'static,
    {
        self.factories.push(TaskFactory {
            name: name.into(),
            factory: Arc::new(factory),
            instances,
            affinity,
        });
    }

    /// Start all tasks and wait for shutdown.
    pub async fn run(self) -> ShutdownResult<()> {
        if self.config.validate_core_allocation {
            self.validate_allocations()?;
        }

        let tasks = self.tasks;
        let factories = self.factories;
        let config = self.config;
        let affinities = self.task_affinities;

        let toplevel_fn = move |subsys: &mut SubsystemHandle| {
            // single instance tasks
            for (i, task) in tasks.into_iter().enumerate() {
                let task_clone = task.clone();
                let task_config = config;
                let affinity = affinities[i].clone();

                subsys.start(SubsystemBuilder::new(
                    task.name(),
                    move |subsys: &mut SubsystemHandle| {
                        let t = task_clone.clone();
                        let name = t.name().to_string();
                        let token = subsys.create_cancellation_token();

                        async move {
                            supervise(
                                name,
                                t,
                                token,
                                task_config.shutdown_on_error,
                                affinity,
                                Some(i),
                            )
                            .await
                        }
                    },
                ));
            }

            // multiple tasks intsance from factories
            for reg in factories {
                let factory = reg.factory.clone();
                let group_name = reg.name.clone();
                let factory_config = config;
                let affinity = reg.affinity.clone();

                for i in 0..reg.instances {
                    let task = (factory)();
                    let task_name = format!("{}-{}", group_name, i);
                    let t = task.clone();
                    let affinity = affinity.clone();

                    subsys.start(SubsystemBuilder::new(
                        task_name.clone(),
                        move |subsys: &mut SubsystemHandle| {
                            let t = t.clone();
                            let token = subsys.create_cancellation_token();

                            async move {
                                supervise(
                                    task_name,
                                    t,
                                    token,
                                    factory_config.shutdown_on_error,
                                    affinity,
                                    Some(i),
                                )
                                .await
                            }
                        },
                    ));
                }
            }

            async {}
        };

        let mut builder = Toplevel::new(toplevel_fn);

        if config.catch_signals {
            builder = builder.catch_signals();
        }

        builder
            .handle_shutdown_requests(config.shutdown_timeout)
            .await
            .map_err(|e| match e {
                GracefulShutdownError::ShutdownTimeout(_) => {
                    ShutdownError::timeout(config.shutdown_timeout)
                }
                GracefulShutdownError::SubsystemsFailed(failures) => {
                    let task_errors = failures
                        .into_iter()
                        .map(|f| {
                            let error_msg = format!("{:?}", f);
                            TaskError::new(
                                f.name().to_string(),
                                TaskErrorKind::Execution {
                                    source: error_msg.into(),
                                },
                            )
                        })
                        .collect();
                    ShutdownError::subsystems_failed(task_errors)
                }
            })
    }

    fn validate_allocations(&self) -> Result<CoreAllocator, ShutdownError> {
        let mut allocator = CoreAllocator::new();

        // Allocate for single instance tasks
        for (i, task) in self.tasks.iter().enumerate() {
            let affinity = &self.task_affinities[i];
            let task_name = task.name();

            if let Err(e) = allocator.allocate(task_name, affinity, None) {
                return Err(ShutdownError::invalid_core_allocation(e));
            }
        }

        // Allocate for factory tasks
        for factory in &self.factories {
            for i in 0..factory.instances {
                let task_name = format!("{}-{}", factory.name, i);

                if let Err(e) = allocator.allocate(&task_name, &factory.affinity, Some(i)) {
                    return Err(ShutdownError::invalid_core_allocation(e));
                }
            }
        }

        // Check for conflicts
        if let Err(errors) = allocator.validate() {
            warn!("Core allocation conflicts detected:");
            for error in &errors {
                warn!("  {}", error);
            }
            warn!("Multiple tasks will share the same CPU core, which may impact performance");
        }

        // Log allocation report
        info!("{}", allocator.get_allocation_report());

        Ok(allocator)
    }
}

async fn supervise(
    task_name: String,
    task: Arc<dyn RunnableTask>,
    token: CancellationToken,
    shutdown_on_error: bool,
    affinity: CoreAffinityConfig,
    instance_index: Option<usize>,
) -> TaskResult<()> {
    if let Some(core_ids) = core_affinity::get_core_ids() {
        match affinity {
            CoreAffinityConfig::None => {}

            CoreAffinityConfig::Fixed(id) => {
                // fixed always works regardless of instance_index
                if let Some(core) = core_ids.into_iter().find(|c| c.id == id) {
                    core_affinity::set_for_current(core);
                    info!(task = %task_name, core = id, "pinned to specific core");
                } else {
                    error!(task = %task_name, core = id, "requested core not available");
                }
            }

            CoreAffinityConfig::Range { start, end } => {
                let range: Vec<_> = core_ids
                    .into_iter()
                    .filter(|c| (start..=end).contains(&c.id))
                    .collect();

                if range.is_empty() {
                    error!(task = %task_name, "no cores in requested range {}-{}", start, end);
                } else if let Some(i) = instance_index {
                    // for multi-instance: distribute across range
                    if let Some(core) = range.get(i % range.len()) {
                        core_affinity::set_for_current(*core);
                        info!(task = %task_name, core = core.id, instance = i, "pinned to core in range");
                    }
                } else {
                    // for single instance: use first core in range
                    if let Some(core) = range.first() {
                        core_affinity::set_for_current(*core);
                        info!(task = %task_name, core = core.id, "single task pinned to first core in range");
                    }
                }
            }

            CoreAffinityConfig::Auto => {
                if let Some(i) = instance_index {
                    // for multi-instance: round-robin across all cores
                    if let Some(core) = core_ids.get(i % core_ids.len()) {
                        core_affinity::set_for_current(*core);
                        info!(task = %task_name, core = core.id, instance = i, "auto-pinned by index");
                    }
                } else {
                    // for single instance: Auto doesn't make sense, warn and skip
                    error!(task = %task_name, "Auto affinity not supported for single instance tasks, use Fixed or Range");
                }
            }
        }
    }

    info!(task = %task_name, "starting subsystem");

    let res = task.run(token).await;
    let _ = task.on_shutdown().await;

    info!(task = %task_name, "subsystem stopped");

    if !shutdown_on_error {
        if let Err(ref e) = res {
            error!(task = %task_name, ?e, "task failed but shutdown_on_error=false");
        }
        Ok(())
    } else {
        res
    }
}

pub type Factory = Arc<dyn Fn() -> Arc<dyn RunnableTask> + Send + Sync>;

pub struct TaskFactory {
    pub name: String,
    pub factory: Factory,
    pub instances: usize,
    pub affinity: CoreAffinityConfig,
}
