use super::resources::ResBuilder;
use crate::internal::common::resources::{
    NumOfNodes, ResourceAmount, ResourceId, ResourceRequestVariants,
};
use crate::internal::messages::worker::TaskRunningMsg;
use crate::internal::server::task::{Task, TaskConfiguration, TaskInput};
use crate::resources::ResourceRequest;
use crate::{Priority, TaskId};
use smallvec::SmallVec;
use std::rc::Rc;

pub struct TaskBuilder {
    id: TaskId,
    inputs: Vec<TaskInput>,
    n_outputs: u32,
    finished_resources: Vec<ResourceRequest>,
    resources_builder: ResBuilder,
    user_priority: Priority,
    crash_limit: u32,
}

impl TaskBuilder {
    pub fn new<T: Into<TaskId>>(id: T) -> TaskBuilder {
        TaskBuilder {
            id: id.into(),
            inputs: Default::default(),
            n_outputs: 0,
            finished_resources: vec![],
            resources_builder: Default::default(),
            user_priority: 0,
            crash_limit: 5,
        }
    }

    pub fn user_priority(mut self, value: Priority) -> TaskBuilder {
        self.user_priority = value;
        self
    }

    pub fn simple_deps(mut self, deps: &[&Task]) -> TaskBuilder {
        self.inputs = deps.iter().map(|&tr| TaskInput::new(tr.id, 0)).collect();
        self
    }

    pub fn task_deps(mut self, deps: &[&Task]) -> TaskBuilder {
        self.inputs = deps
            .iter()
            .map(|&tr| TaskInput::new_task_dependency(tr.id))
            .collect();
        self
    }

    pub fn outputs(mut self, value: u32) -> TaskBuilder {
        self.n_outputs = value;
        self
    }

    pub fn next_resources(mut self) -> TaskBuilder {
        self.finished_resources
            .push(self.resources_builder.finish());
        self.resources_builder = ResBuilder::default();
        self
    }

    pub fn resources(mut self, resources: ResBuilder) -> TaskBuilder {
        self.resources_builder = resources;
        self
    }

    pub fn n_nodes(mut self, count: NumOfNodes) -> TaskBuilder {
        self.resources_builder = self.resources_builder.n_nodes(count);
        self
    }

    pub fn cpus_compact(mut self, count: ResourceAmount) -> TaskBuilder {
        self.resources_builder = self.resources_builder.cpus(count);
        self
    }

    pub fn time_request(mut self, time_s: u64) -> TaskBuilder {
        self.resources_builder = self.resources_builder.min_time_secs(time_s);
        self
    }

    pub fn add_resource<Id: Into<ResourceId>>(
        mut self,
        id: Id,
        amount: ResourceAmount,
    ) -> TaskBuilder {
        self.resources_builder = self.resources_builder.add(id, amount);
        self
    }

    pub fn build(self) -> Task {
        let last_resource = self.resources_builder.finish();
        let mut resources: SmallVec<[ResourceRequest; 1]> = self.finished_resources.into();
        resources.push(last_resource);
        for rq in &resources {
            rq.validate().unwrap();
        }
        let resources = ResourceRequestVariants::new(resources);
        Task::new(
            self.id,
            self.inputs.into_iter().collect(),
            Rc::new(TaskConfiguration {
                resources,
                n_outputs: self.n_outputs,
                time_limit: None,
                user_priority: self.user_priority,
                crash_limit: self.crash_limit,
            }),
            Default::default(),
            false,
            false,
        )
    }
}

pub fn task<T: Into<TaskId>>(id: T) -> Task {
    TaskBuilder::new(id.into()).outputs(1).build()
}

pub fn task_with_deps<T: Into<TaskId>>(id: T, deps: &[&Task], n_outputs: u32) -> Task {
    TaskBuilder::new(id.into())
        .simple_deps(deps)
        .outputs(n_outputs)
        .build()
}

pub fn task_running_msg<T: Into<TaskId>>(task_id: T) -> TaskRunningMsg {
    TaskRunningMsg {
        id: task_id.into(),
        context: Default::default(),
    }
}
