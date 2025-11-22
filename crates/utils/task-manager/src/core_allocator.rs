use std::collections::HashMap;

/// Core pinning config for a task and/or worker group.
#[derive(Debug, Clone)]
pub enum CoreAffinityConfig {
    /// No pinning. Let OS schedule it freely.
    None,

    /// Pin to specific core by id.
    Fixed(usize),

    /// Pin multiple workers to range of cores (e.g., [2..=5]).
    Range { start: usize, end: usize },

    /// Assign automatically to available cores.
    Auto,
}

#[derive(Debug)]
pub struct CoreAllocator {
    /// Mapping of core id > task name.
    core_usage: HashMap<usize, Vec<String>>,
    /// Available core for use.
    available_cores: Vec<usize>,
    /// Next core index for auto-allocation.
    next_auto_core: usize,
}

impl Default for CoreAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreAllocator {
    pub fn new() -> Self {
        let available_cores = core_affinity::get_core_ids()
            .map(|cores| cores.iter().map(|c| c.id).collect())
            .unwrap_or_default();

        Self {
            core_usage: HashMap::new(),
            available_cores,
            next_auto_core: 0,
        }
    }

    /// Allocate a core for a task
    pub fn allocate(
        &mut self,
        task_name: &str,
        affinity: &CoreAffinityConfig,
        instance_index: Option<usize>,
    ) -> Result<Option<usize>, String> {
        if self.available_cores.is_empty() {
            return Ok(None); // Skip as no cores are available.
        }

        let core_id = match affinity {
            CoreAffinityConfig::None => return Ok(None),

            CoreAffinityConfig::Fixed(id) => {
                if !self.available_cores.contains(id) {
                    return Err(format!(
                        "Core {} requested by task '{}' is not available. Available cores: {:?}",
                        id, task_name, self.available_cores
                    ));
                }
                *id
            }

            CoreAffinityConfig::Range { start, end } => {
                let range: Vec<usize> = self
                    .available_cores
                    .iter()
                    .filter(|c| (*start..=*end).contains(c))
                    .copied()
                    .collect();

                if range.is_empty() {
                    return Err(format!(
                        "No cores available in range {}-{} for task '{}'. Available cores: {:?}",
                        start, end, task_name, self.available_cores
                    ));
                }

                if let Some(i) = instance_index {
                    // Multi-instance: distribute across range
                    range[i % range.len()]
                } else {
                    // Single instance: use first core in range
                    range[0]
                }
            }

            CoreAffinityConfig::Auto => {
                if instance_index.is_none() {
                    return Err(format!(
                        "Auto affinity not supported for single instance task '{}'. Use Fixed or Range instead.",
                        task_name
                    ));
                }

                // Round-robin allocation
                let core_id =
                    self.available_cores[self.next_auto_core % self.available_cores.len()];
                self.next_auto_core += 1;

                core_id
            }
        };

        // Track usage
        self.core_usage
            .entry(core_id)
            .or_default()
            .push(task_name.to_string());

        Ok(Some(core_id))
    }

    /// Get a report of core allocations.
    pub fn get_allocation_report(&self) -> String {
        if self.core_usage.is_empty() {
            return "No core allocations made".to_string();
        }

        let mut report = String::from("Core Allocation Report:\n");
        let mut cores: Vec<_> = self.core_usage.keys().collect();
        cores.sort();

        for core_id in cores {
            if let Some(tasks) = self.core_usage.get(core_id) {
                report.push_str(&format!("  Core {}: {} task(s)\n", core_id, tasks.len()));
                for task in tasks {
                    report.push_str(&format!("    - {}\n", task));
                }
            }
        }

        report
    }

    /// Check for conflicts (multiple tasks on same core).
    pub fn get_conflicts(&self) -> Vec<(usize, Vec<String>)> {
        self.core_usage
            .iter()
            .filter(|(_, tasks)| tasks.len() > 1)
            .map(|(core_id, tasks)| (*core_id, tasks.clone()))
            .collect()
    }

    /// Validate allocation plan and return warnings/errors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let conflicts = self.get_conflicts();

        if conflicts.is_empty() {
            return Ok(());
        }

        let errors: Vec<String> = conflicts
            .into_iter()
            .map(|(core_id, tasks)| {
                format!(
                    "Core {} conflict: {} tasks pinned to same core: {}",
                    core_id,
                    tasks.len(),
                    tasks.join(", ")
                )
            })
            .collect();

        Err(errors)
    }
}
