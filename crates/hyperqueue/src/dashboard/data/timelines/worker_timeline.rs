use crate::dashboard::data::time_based_vec::{ItemWithTime, TimeBasedVec};
use crate::dashboard::data::time_interval::TimeRange;
use crate::dashboard::data::Time;
use crate::server::event::events::MonitoringEventPayload;
use crate::server::event::MonitoringEvent;
use crate::WorkerId;
use std::time::SystemTime;
use tako::gateway::LostWorkerReason;
use tako::worker::{WorkerConfiguration, WorkerOverview};
use tako::Map;

#[derive(Clone)]
pub struct WorkerDisconnectInfo {
    pub reason: LostWorkerReason,
    pub time: Time,
}

pub struct WorkerRecord {
    id: WorkerId,
    connection_time: SystemTime,
    worker_config: WorkerConfiguration,
    worker_overviews: TimeBasedVec<WorkerOverview>,

    disconnect_info: Option<WorkerDisconnectInfo>,
}

impl WorkerRecord {
    pub fn set_loss_details(&mut self, loss_time: SystemTime, loss_reason: LostWorkerReason) {
        self.disconnect_info = Some(WorkerDisconnectInfo {
            reason: loss_reason,
            time: loss_time,
        });
    }
}

/// Stores information about the workers at different times
#[derive(Default)]
pub struct WorkerTimeline {
    workers: Map<WorkerId, WorkerRecord>,
}

impl WorkerTimeline {
    /// Assumes that `events` are sorted by time.
    pub fn handle_new_events(&mut self, events: &[MonitoringEvent]) {
        for event in events {
            match &event.payload {
                MonitoringEventPayload::WorkerConnected(id, info) => {
                    self.workers.insert(
                        *id,
                        WorkerRecord {
                            id: *id,
                            connection_time: event.time,
                            worker_config: *info.clone(),
                            worker_overviews: Default::default(),
                            disconnect_info: None,
                        },
                    );
                }
                MonitoringEventPayload::WorkerLost(lost_id, reason) => {
                    if let Some(worker) = self.workers.get_mut(lost_id) {
                        worker.set_loss_details(event.time, reason.clone());
                    }
                }
                MonitoringEventPayload::WorkerOverviewReceived(overview) => {
                    if let Some(worker) = self.workers.get_mut(&overview.id) {
                        worker.worker_overviews.push(event.time, overview.clone());
                    }
                }
                _ => {}
            }
        }
    }

    pub fn get_worker_ids(&self) -> impl Iterator<Item = WorkerId> + '_ {
        self.workers.values().map(|worker| worker.id)
    }

    pub fn get_worker_config_for(&self, worker_id: WorkerId) -> Option<&WorkerConfiguration> {
        self.workers.get(&worker_id).map(|w| &w.worker_config)
    }

    pub fn get_worker_disconnect_info(&self, worker_id: WorkerId) -> Option<WorkerDisconnectInfo> {
        self.workers
            .get(&worker_id)
            .and_then(|w| w.disconnect_info.clone())
    }

    pub fn get_connected_worker_ids_at(
        &self,
        time: SystemTime,
    ) -> impl Iterator<Item = WorkerId> + '_ {
        self.workers
            .iter()
            .filter(move |(_, worker)| {
                let has_started = worker.connection_time <= time;
                let has_finished = match worker.disconnect_info {
                    Some(WorkerDisconnectInfo {
                        time: lost_time, ..
                    }) => lost_time <= time,
                    None => false,
                };
                has_started && !has_finished
            })
            .map(|(worker_id, _)| *worker_id)
    }

    pub fn get_worker_overview_at(
        &self,
        worker_id: WorkerId,
        time: SystemTime,
    ) -> Option<&ItemWithTime<WorkerOverview>> {
        self.workers
            .get(&worker_id)
            .and_then(|worker| worker.worker_overviews.get_most_recent_at(time))
    }

    pub fn get_worker_overviews_at(
        &self,
        worker_id: WorkerId,
        range: TimeRange,
    ) -> Option<&[ItemWithTime<WorkerOverview>]> {
        self.workers
            .get(&worker_id)
            .map(|worker| worker.worker_overviews.get_time_range(range))
    }
}
