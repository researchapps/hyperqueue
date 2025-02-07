use crate::client::resources::parse_allocation_request;
use crate::common::arraydef::IntArray;
use crate::common::arrayparser::parse_array;
use crate::common::error::HqError;
use crate::common::utils::time::parse_human_time;
use crate::{JobTaskCount, JobTaskId};
use bstr::BString;
use serde::{Deserialize, Deserializer};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::time::Duration;
use tako::gateway::{ResourceRequest, ResourceRequestEntries, ResourceRequestEntry};
use tako::resources::{AllocationRequest, NumOfNodes};
use tako::{Map, Priority};

#[derive(Deserialize)]
#[serde(untagged)]
pub enum IntOrString {
    Int(u64),
    String(String),
}

#[derive(Default, Debug, Deserialize)]
pub enum PinMode {
    #[default]
    #[serde(rename = "none")]
    None,
    #[serde(rename = "taskset")]
    TaskSet,
    #[serde(rename = "omp")]
    OpenMP,
}

impl PinMode {
    pub fn into(&self) -> crate::transfer::messages::PinMode {
        match self {
            PinMode::None => crate::transfer::messages::PinMode::None,
            PinMode::TaskSet => crate::transfer::messages::PinMode::TaskSet,
            PinMode::OpenMP => crate::transfer::messages::PinMode::OpenMP,
        }
    }
}

fn deserialize_human_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    parse_human_time(&buf).map_err(serde::de::Error::custom)
}

fn deserialize_resource_entries<'de, D>(deserializer: D) -> Result<ResourceRequestEntries, D::Error>
where
    D: Deserializer<'de>,
{
    let tmp = Map::<String, IntOrString>::deserialize(deserializer)?;

    let mut result = ResourceRequestEntries::new();
    for (k, v) in tmp {
        let policy = match v {
            IntOrString::Int(n) => AllocationRequest::Compact(n),
            IntOrString::String(s) => {
                parse_allocation_request(&s).map_err(serde::de::Error::custom)?
            }
        };
        result.push(ResourceRequestEntry {
            resource: k,
            policy,
        })
    }
    Ok(result)
}

fn deserialize_human_duration_opt<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = Option::<String>::deserialize(deserializer)?;

    if let Some(b) = buf {
        parse_human_time(&b)
            .map(Some)
            .map_err(serde::de::Error::custom)
    } else {
        Ok(None)
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequestDef {
    #[serde(default)]
    pub n_nodes: NumOfNodes,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_human_duration")]
    pub time_request: Duration,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_resource_entries")]
    pub resources: ResourceRequestEntries,
}

impl ResourceRequestDef {
    pub fn into_request(self) -> ResourceRequest {
        ResourceRequest {
            n_nodes: self.n_nodes,
            resources: self.resources,
            min_time: self.time_request,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TaskConfigDef {
    pub command: Vec<String>,

    #[serde(default)]
    pub env: Map<BString, BString>,

    #[serde(default)]
    pub request: SmallVec<[ResourceRequestDef; 1]>,

    #[serde(default)]
    pub stdout: Option<String>,

    #[serde(default)]
    pub stderr: Option<String>,

    #[serde(default)]
    pub cwd: Option<String>,

    #[serde(default)]
    pub pin: PinMode,

    #[serde(default)]
    pub task_dir: bool,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_human_duration_opt")]
    pub time_limit: Option<Duration>,

    #[serde(default)]
    pub priority: Priority,

    #[serde(default)]
    pub crash_limit: u32,

    #[serde(default)]
    pub stdin: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TaskDef {
    pub id: Option<JobTaskId>,

    #[serde(flatten)]
    pub config: TaskConfigDef,

    #[serde(default)]
    pub deps: Vec<JobTaskId>,
}

fn deserialize_array_opt<'de, D>(deserializer: D) -> Result<Option<IntArray>, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = Option::<String>::deserialize(deserializer)?;
    buf.map(|b| parse_array(&b).map_err(serde::de::Error::custom))
        .transpose()
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArrayDef {
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_array_opt")]
    pub ids: Option<IntArray>,

    #[serde(default)]
    pub entries: Vec<String>,

    #[serde(flatten)]
    pub config: TaskConfigDef,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct JobDef {
    #[serde(default)]
    pub name: String,

    pub max_fails: Option<JobTaskCount>,

    #[serde(default)]
    #[serde(rename = "task")]
    pub tasks: Vec<TaskDef>,

    pub array: Option<ArrayDef>,

    pub stream_log: Option<PathBuf>,
}

impl JobDef {
    pub fn validate(&self) -> crate::Result<()> {
        if self.tasks.is_empty() && self.array.is_none() {
            return Err(HqError::DeserializationError("No tasks defined".into()));
        }

        if let Some(array) = &self.array {
            if !self.tasks.is_empty() {
                return Err(HqError::DeserializationError(
                    "Definition of array job and individual task cannot be mixed".into(),
                ));
            }
            if array.ids.is_none() && array.entries.is_empty() {
                return Err(HqError::DeserializationError(
                    "One of attributes of array must be defined: 'ids', 'entries'".into(),
                ));
            }
            if let Some(ids) = &array.ids {
                if !array.entries.is_empty() && ids.id_count() as usize != array.entries.len() {
                    return Err(HqError::DeserializationError(
                        "Items 'ids' and 'entries' does not match".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn parse(str: &str) -> crate::Result<JobDef> {
        let jdef: JobDef = toml::from_str(str)?;
        jdef.validate()?;
        Ok(jdef)
    }
}

#[cfg(test)]
mod test {
    use crate::client::commands::submit::defs::JobDef;
    use crate::common::error::HqError;
    use bstr::{BString, ByteSlice};

    #[test]
    fn test_read_minimal_def() {
        let jdef = JobDef::parse(
            r#"
        [[task]]
        command = ["sleep", "1"]
        "#,
        )
        .unwrap();
        assert!(jdef.name.is_empty());
        assert_eq!(jdef.tasks.len(), 1);
        assert!(jdef.tasks[0].id.is_none());
        assert_eq!(
            jdef.tasks[0].config.command,
            vec!["sleep".to_string(), "1".to_string()]
        );
    }

    #[test]
    fn test_validate_no_task_def() {
        let r = JobDef::parse(
            r#"
        name = 'myjob'
        "#,
        );
        assert!(matches!(r, Err(HqError::DeserializationError(_))))
        //assert!(r.is_err());
    }

    #[test]
    fn test_unknown_fields() {
        let r = JobDef::parse(
            r#"
        [[task]]
        command = ["sleep", "1"]
        some_unknown_field_xxx = "123"
        "#,
        );
        assert!(matches!(r, Err(HqError::DeserializationError(_))))
        //assert!(r.is_err());
    }

    #[test]
    fn test_parse_env1() {
        let r = JobDef::parse(
            r#"
        [[task]]
        command = ["sleep", "1"]
        env = {"ABC" = "abc", "XYZ" = "55"}
        "#,
        )
        .unwrap();
        assert_eq!(r.tasks[0].config.env.len(), 2);
        assert_eq!(
            r.tasks[0]
                .config
                .env
                .get(BString::from("ABC").as_bstr())
                .unwrap(),
            "abc"
        );
        assert_eq!(
            r.tasks[0]
                .config
                .env
                .get(BString::from("XYZ").as_bstr())
                .unwrap(),
            "55"
        );
    }

    #[test]
    fn test_parse_env2() {
        let r = JobDef::parse(
            r#"
        [[task]]
        command = ["sleep", "1"]
            [task.env]
            "ABC" = "abc"
            "XYZ" = "55"
        "#,
        )
        .unwrap();
        assert_eq!(r.tasks[0].config.env.len(), 2);
        assert_eq!(
            r.tasks[0]
                .config
                .env
                .get(BString::from("ABC").as_bstr())
                .unwrap(),
            "abc"
        );
        assert_eq!(
            r.tasks[0]
                .config
                .env
                .get(BString::from("XYZ").as_bstr())
                .unwrap(),
            "55"
        );
    }
}
