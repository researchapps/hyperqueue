use crate::internal::common::resources::{
    CpuId, GenericResourceAmount, GenericResourceIndex, NumOfCpus,
};
use crate::internal::common::Set;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GenericResourceDescriptorKind {
    List {
        values: Vec<GenericResourceIndex>,
    },
    Range {
        start: GenericResourceIndex,
        end: GenericResourceIndex,
    },
    // TODO: Named(Vec<String>),
    Sum {
        size: GenericResourceAmount,
    },
}

impl GenericResourceDescriptorKind {
    pub fn size(&self) -> GenericResourceAmount {
        match self {
            GenericResourceDescriptorKind::List { values } => values.len() as GenericResourceAmount,
            GenericResourceDescriptorKind::Range { start, end } if end >= start => {
                (end.as_num() + 1 - start.as_num()) as u64
            }
            GenericResourceDescriptorKind::Range { .. } => 0,
            GenericResourceDescriptorKind::Sum { size } => *size,
        }
    }
}

impl std::fmt::Display for GenericResourceDescriptorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenericResourceDescriptorKind::List { values } => {
                write!(
                    f,
                    "List({})",
                    values
                        .iter()
                        .map(|idx| idx.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
            GenericResourceDescriptorKind::Range { start, end } => {
                write!(f, "Range({start}-{end})")
            }
            GenericResourceDescriptorKind::Sum { size } => write!(f, "Sum({size})"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GenericResourceDescriptor {
    pub name: String,
    pub kind: GenericResourceDescriptorKind,
}

impl GenericResourceDescriptor {
    pub fn range<Index: Into<GenericResourceIndex>>(name: &str, start: Index, end: Index) -> Self {
        GenericResourceDescriptor {
            name: name.to_string(),
            kind: GenericResourceDescriptorKind::Range {
                start: start.into(),
                end: end.into(),
            },
        }
    }
    pub fn sum(name: &str, size: GenericResourceAmount) -> Self {
        GenericResourceDescriptor {
            name: name.to_string(),
            kind: GenericResourceDescriptorKind::Sum { size },
        }
    }
}

/// (Node0(Cpu0, Cpu1), Node1(Cpu2, Cpu3), ...)
pub type CpusDescriptor = Vec<Vec<CpuId>>;

pub fn cpu_descriptor_from_socket_size(
    n_sockets: NumOfCpus,
    n_cpus_per_socket: NumOfCpus,
) -> CpusDescriptor {
    let mut cpu_id_counter = 0;
    (0..n_sockets)
        .map(|_| {
            (0..n_cpus_per_socket)
                .map(|_| {
                    let id = cpu_id_counter;
                    cpu_id_counter += 1;
                    id.into()
                })
                .collect::<Vec<CpuId>>()
        })
        .collect()
}

/// Most precise description of request provided by a worker (without time resource)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourceDescriptor {
    pub cpus: CpusDescriptor,
    pub generic: Vec<GenericResourceDescriptor>,
}

impl ResourceDescriptor {
    pub fn new(cpus: CpusDescriptor, mut generic: Vec<GenericResourceDescriptor>) -> Self {
        generic.sort_by(|x, y| x.name.cmp(&y.name));

        ResourceDescriptor { cpus, generic }
    }

    pub fn full_describe(&self) -> String {
        self.cpus
            .iter()
            .map(|socket| {
                format!(
                    "[{}]",
                    socket
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.cpus.is_empty() || !self.cpus.iter().all(|g| !g.is_empty()) {
            return Err(crate::Error::GenericError("Invalid number of cpus".into()));
        }
        let s: Set<CpuId> = self.cpus.iter().flatten().copied().collect();
        if s.len() != self.cpus.iter().flatten().count() {
            return Err(crate::Error::GenericError(
                "Same CPU id in two sockets".into(),
            ));
        }

        let s: Set<String> = self.generic.iter().map(|g| g.name.clone()).collect();
        if s.len() != self.generic.len() {
            return Err(crate::Error::GenericError(
                "Same resource defined twice".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::common::index::AsIdVec;

    impl ResourceDescriptor {
        pub fn simple(n_cpus: NumOfCpus) -> Self {
            Self::new(
                cpu_descriptor_from_socket_size(1, n_cpus),
                Default::default(),
            )
        }
    }

    #[test]
    fn test_resources_to_describe() {
        let d = ResourceDescriptor::new(vec![vec![0].to_ids()], Vec::new());
        assert_eq!(&d.full_describe(), "[0]");

        let d = ResourceDescriptor::new(
            vec![vec![0, 1, 2, 4].to_ids(), vec![10, 11, 12, 14].to_ids()],
            Vec::new(),
        );
        assert_eq!(&d.full_describe(), "[0, 1, 2, 4], [10, 11, 12, 14]");
    }
}
