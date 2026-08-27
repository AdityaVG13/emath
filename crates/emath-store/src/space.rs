//! Local named views over a shared immutable object graph.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use emath_core::{MergeId, ObjectId, SnapshotId};

use crate::ObjectGraph;
use crate::object_graph::frame;

const SNAPSHOT_SCHEMA_V1: &str = "emath.store.snapshot.v1";
const MERGE_SCHEMA_V1: &str = "emath.store.merge.v1";

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpacePolicy {
    pub lens: Option<String>,
    pub provider: Option<String>,
    pub trust: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Space {
    name: String,
    objects: Arc<ObjectGraph>,
    aliases: BTreeMap<String, BTreeSet<ObjectId>>,
    policies: BTreeSet<SpacePolicy>,
    lock_root: Option<SnapshotId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceSnapshot {
    pub id: SnapshotId,
    pub space_name: String,
    pub aliases: BTreeMap<String, BTreeSet<ObjectId>>,
    pub policies: BTreeSet<SpacePolicy>,
    pub parent_lock: Option<SnapshotId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryLock {
    pub root: SnapshotId,
    pub dependencies: BTreeSet<ObjectId>,
    pub revoked: BTreeSet<ObjectId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reconciliation {
    Choose {
        alias: String,
        selected: ObjectId,
    },
    Rename {
        alias: String,
        object: ObjectId,
        new_alias: String,
    },
    Morphism {
        source: ObjectId,
        target: ObjectId,
    },
    ProveEquivalent {
        left: ObjectId,
        right: ObjectId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeAction {
    pub reconciliation_object: ObjectId,
    pub operation: Reconciliation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeReceipt {
    pub id: MergeId,
    pub ancestor: SnapshotId,
    pub left: SnapshotId,
    pub right: SnapshotId,
    pub actions: Vec<MergeAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceError {
    EmptyName,
    EmptyAlias,
    MissingObject(ObjectId),
    RevokedObject(ObjectId),
    DifferentObjectGraph,
    NoCommonAncestor,
    AliasDoesNotContain { alias: String, object: ObjectId },
}

impl fmt::Display for SpaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("space name must not be empty"),
            Self::EmptyAlias => formatter.write_str("space alias must not be empty"),
            Self::MissingObject(id) => write!(formatter, "space references missing object `{id}`"),
            Self::RevokedObject(id) => write!(formatter, "space references revoked object `{id}`"),
            Self::DifferentObjectGraph => {
                formatter.write_str("spaces do not share the same immutable object graph")
            }
            Self::NoCommonAncestor => {
                formatter.write_str("spaces do not name the supplied common snapshot ancestor")
            }
            Self::AliasDoesNotContain { alias, object } => {
                write!(
                    formatter,
                    "alias `{alias}` does not contain object `{object}`"
                )
            }
        }
    }
}

impl std::error::Error for SpaceError {}

impl Space {
    pub fn new(name: impl Into<String>, objects: Arc<ObjectGraph>) -> Result<Self, SpaceError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(SpaceError::EmptyName);
        }
        Ok(Self {
            name,
            objects,
            aliases: BTreeMap::new(),
            policies: BTreeSet::from([SpacePolicy::default()]),
            lock_root: None,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn policy(&self) -> &SpacePolicy {
        self.policies
            .first()
            .expect("every space has at least one policy")
    }

    pub fn policies(&self) -> impl Iterator<Item = &SpacePolicy> {
        self.policies.iter()
    }

    pub fn set_policy(&mut self, policy: SpacePolicy) {
        self.policies.clear();
        self.policies.insert(policy);
    }

    pub fn set_lock_root(&mut self, root: Option<SnapshotId>) {
        self.lock_root = root;
    }

    pub fn bind_alias(
        &mut self,
        alias: impl Into<String>,
        object: ObjectId,
    ) -> Result<bool, SpaceError> {
        let alias = alias.into();
        if alias.trim().is_empty() {
            return Err(SpaceError::EmptyAlias);
        }
        if self.objects.object(&object).is_none() {
            return Err(SpaceError::MissingObject(object));
        }
        Ok(self.aliases.entry(alias).or_default().insert(object))
    }

    #[must_use]
    pub fn alias(&self, alias: &str) -> Option<&BTreeSet<ObjectId>> {
        self.aliases.get(alias)
    }

    pub fn branch(&self, name: impl Into<String>) -> Result<Self, SpaceError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(SpaceError::EmptyName);
        }
        Ok(Self {
            name,
            objects: Arc::clone(&self.objects),
            aliases: self.aliases.clone(),
            policies: self.policies.clone(),
            lock_root: self.lock_root.clone(),
        })
    }

    #[must_use]
    pub fn shares_objects_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.objects, &other.objects)
    }

    pub fn snapshot(&self) -> Result<SpaceSnapshot, SpaceError> {
        for objects in self.aliases.values() {
            for object in objects {
                if self.objects.object(object).is_none() {
                    return Err(SpaceError::MissingObject(object.clone()));
                }
            }
        }
        let id = snapshot_id(&self.aliases, &self.policies, self.lock_root.as_ref());
        Ok(SpaceSnapshot {
            id,
            space_name: self.name.clone(),
            aliases: self.aliases.clone(),
            policies: self.policies.clone(),
            parent_lock: self.lock_root.clone(),
        })
    }

    pub fn semantic_merge(
        name: impl Into<String>,
        ancestor: &SpaceSnapshot,
        left: &Self,
        right: &Self,
        actions: Vec<MergeAction>,
    ) -> Result<(Self, MergeReceipt), SpaceError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(SpaceError::EmptyName);
        }
        if !Arc::ptr_eq(&left.objects, &right.objects) {
            return Err(SpaceError::DifferentObjectGraph);
        }
        if left.lock_root.as_ref() != Some(&ancestor.id)
            || right.lock_root.as_ref() != Some(&ancestor.id)
        {
            return Err(SpaceError::NoCommonAncestor);
        }

        let mut aliases = left.aliases.clone();
        for (alias, objects) in &right.aliases {
            aliases
                .entry(alias.clone())
                .or_default()
                .extend(objects.iter().cloned());
        }
        let mut policies = left.policies.clone();
        policies.extend(right.policies.iter().cloned());
        let mut merged = Self {
            name,
            objects: Arc::clone(&left.objects),
            aliases,
            policies,
            lock_root: Some(ancestor.id.clone()),
        };
        for action in &actions {
            merged.apply_reconciliation(action)?;
        }
        let left_snapshot = left.snapshot()?.id;
        let right_snapshot = right.snapshot()?.id;
        let id = merge_id(&ancestor.id, &left_snapshot, &right_snapshot, &actions);
        Ok((
            merged,
            MergeReceipt {
                id,
                ancestor: ancestor.id.clone(),
                left: left_snapshot,
                right: right_snapshot,
                actions,
            },
        ))
    }

    fn apply_reconciliation(&mut self, action: &MergeAction) -> Result<(), SpaceError> {
        if self.objects.object(&action.reconciliation_object).is_none() {
            return Err(SpaceError::MissingObject(
                action.reconciliation_object.clone(),
            ));
        }
        match &action.operation {
            Reconciliation::Choose { alias, selected } => {
                let objects =
                    self.aliases
                        .get_mut(alias)
                        .ok_or_else(|| SpaceError::AliasDoesNotContain {
                            alias: alias.clone(),
                            object: selected.clone(),
                        })?;
                if !objects.contains(selected) {
                    return Err(SpaceError::AliasDoesNotContain {
                        alias: alias.clone(),
                        object: selected.clone(),
                    });
                }
                objects.clear();
                objects.insert(selected.clone());
            }
            Reconciliation::Rename {
                alias,
                object,
                new_alias,
            } => {
                if new_alias.trim().is_empty() {
                    return Err(SpaceError::EmptyAlias);
                }
                let objects =
                    self.aliases
                        .get_mut(alias)
                        .ok_or_else(|| SpaceError::AliasDoesNotContain {
                            alias: alias.clone(),
                            object: object.clone(),
                        })?;
                if !objects.remove(object) {
                    return Err(SpaceError::AliasDoesNotContain {
                        alias: alias.clone(),
                        object: object.clone(),
                    });
                }
                self.aliases
                    .entry(new_alias.clone())
                    .or_default()
                    .insert(object.clone());
            }
            Reconciliation::Morphism { source, target }
            | Reconciliation::ProveEquivalent {
                left: source,
                right: target,
            } => {
                for object in [source, target] {
                    if self.objects.object(object).is_none() {
                        return Err(SpaceError::MissingObject(object.clone()));
                    }
                }
            }
        }
        Ok(())
    }
}

impl LibraryLock {
    #[must_use]
    pub fn from_snapshot(
        snapshot: &SpaceSnapshot,
        dependencies: impl IntoIterator<Item = ObjectId>,
    ) -> Self {
        let mut all_dependencies = snapshot
            .aliases
            .values()
            .flat_map(BTreeSet::iter)
            .cloned()
            .collect::<BTreeSet<_>>();
        all_dependencies.extend(dependencies);
        Self {
            root: snapshot.id.clone(),
            dependencies: all_dependencies,
            revoked: BTreeSet::new(),
        }
    }

    pub fn verify(&self, objects: &ObjectGraph) -> Result<(), SpaceError> {
        for dependency in &self.dependencies {
            if objects.object(dependency).is_none() {
                return Err(SpaceError::MissingObject(dependency.clone()));
            }
            if self.revoked.contains(dependency) {
                return Err(SpaceError::RevokedObject(dependency.clone()));
            }
        }
        Ok(())
    }

    pub fn revoke(&mut self, object: ObjectId) {
        self.revoked.insert(object);
    }
}

fn snapshot_id(
    aliases: &BTreeMap<String, BTreeSet<ObjectId>>,
    policies: &BTreeSet<SpacePolicy>,
    parent_lock: Option<&SnapshotId>,
) -> SnapshotId {
    let mut bytes = Vec::new();
    frame(&mut bytes, SNAPSHOT_SCHEMA_V1.as_bytes());
    frame(
        &mut bytes,
        &u64::try_from(aliases.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for (alias, objects) in aliases {
        frame(&mut bytes, alias.as_bytes());
        frame(
            &mut bytes,
            &u64::try_from(objects.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        for object in objects {
            frame(&mut bytes, object.as_str().as_bytes());
        }
    }
    frame(
        &mut bytes,
        &u64::try_from(policies.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for policy in policies {
        for policy_value in [&policy.lens, &policy.provider, &policy.trust] {
            match policy_value {
                Some(value) => {
                    bytes.push(1);
                    frame(&mut bytes, value.as_bytes());
                }
                None => bytes.push(0),
            }
        }
    }
    match parent_lock {
        Some(root) => {
            bytes.push(1);
            frame(&mut bytes, root.as_str().as_bytes());
        }
        None => bytes.push(0),
    }
    SnapshotId::from_bytes(&bytes)
}

fn merge_id(
    ancestor: &SnapshotId,
    left: &SnapshotId,
    right: &SnapshotId,
    actions: &[MergeAction],
) -> MergeId {
    let mut bytes = Vec::new();
    frame(&mut bytes, MERGE_SCHEMA_V1.as_bytes());
    frame(&mut bytes, ancestor.as_str().as_bytes());
    let mut sides = [left.as_str(), right.as_str()];
    sides.sort_unstable();
    for side in sides {
        frame(&mut bytes, side.as_bytes());
    }
    frame(
        &mut bytes,
        &u64::try_from(actions.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for action in actions {
        frame(&mut bytes, action.reconciliation_object.as_str().as_bytes());
        match &action.operation {
            Reconciliation::Choose { alias, selected } => {
                bytes.push(0);
                frame(&mut bytes, alias.as_bytes());
                frame(&mut bytes, selected.as_str().as_bytes());
            }
            Reconciliation::Rename {
                alias,
                object,
                new_alias,
            } => {
                bytes.push(1);
                frame(&mut bytes, alias.as_bytes());
                frame(&mut bytes, object.as_str().as_bytes());
                frame(&mut bytes, new_alias.as_bytes());
            }
            Reconciliation::Morphism { source, target } => {
                bytes.push(2);
                frame(&mut bytes, source.as_str().as_bytes());
                frame(&mut bytes, target.as_str().as_bytes());
            }
            Reconciliation::ProveEquivalent { left, right } => {
                bytes.push(3);
                frame(&mut bytes, left.as_str().as_bytes());
                frame(&mut bytes, right.as_str().as_bytes());
            }
        }
    }
    MergeId::from_bytes(&bytes)
}
