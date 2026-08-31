//! Immutable mathematical objects and typed relations.

use std::collections::BTreeMap;
use std::fmt;

use emath_core::{EvidenceId, MeaningId, ObjectId, RelationId};

const OBJECT_SCHEMA_V1: &str = "emath.store.object.v1";
const RELATION_SCHEMA_V1: &str = "emath.store.relation.v1";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectKind {
    Cell,
    Theory,
    Proof,
    Method,
    Lesson,
    Recipe,
    Custom(String),
}

impl ObjectKind {
    pub(crate) fn canonical_name(&self) -> String {
        match self {
            Self::Cell => "cell".to_string(),
            Self::Theory => "theory".to_string(),
            Self::Proof => "proof".to_string(),
            Self::Method => "method".to_string(),
            Self::Lesson => "lesson".to_string(),
            Self::Recipe => "recipe".to_string(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }

    fn validate(&self) -> Result<(), StoreGraphError> {
        if matches!(self, Self::Custom(name) if name.trim().is_empty()) {
            return Err(StoreGraphError::EmptyCustomKind);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelationKind {
    DependsOn,
    Proves,
    EquivalentTo,
    Refines,
    Implements,
    Uses,
    Custom(String),
}

impl RelationKind {
    fn canonical_name(&self) -> String {
        match self {
            Self::DependsOn => "depends-on".to_string(),
            Self::Proves => "proves".to_string(),
            Self::EquivalentTo => "equivalent-to".to_string(),
            Self::Refines => "refines".to_string(),
            Self::Implements => "implements".to_string(),
            Self::Uses => "uses".to_string(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }

    fn validate(&self) -> Result<(), StoreGraphError> {
        if matches!(self, Self::Custom(name) if name.trim().is_empty()) {
            return Err(StoreGraphError::EmptyCustomRelation);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelationScope {
    Global,
    Namespace(String),
    Object(ObjectId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDraft {
    pub kind: ObjectKind,
    pub meaning_id: MeaningId,
    /// Canonical kind-specific bytes. Presentation does not belong here.
    pub semantic_payload: Vec<u8>,
    /// Human-facing text, excluded from `ObjectID`.
    pub presentation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryObject {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub meaning_id: MeaningId,
    pub semantic_payload: Vec<u8>,
    pub presentation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationDraft {
    pub kind: RelationKind,
    pub source: ObjectId,
    pub target: ObjectId,
    pub scope: RelationScope,
    pub assumptions: Vec<MeaningId>,
    pub authority: Option<String>,
    pub evidence: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub id: RelationId,
    pub kind: RelationKind,
    pub source: ObjectId,
    pub target: ObjectId,
    pub scope: RelationScope,
    pub assumptions: Vec<MeaningId>,
    pub authority: Option<String>,
    pub evidence: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreGraphError {
    EmptyCustomKind,
    EmptyCustomRelation,
    EmptyNamespace,
    MissingObject(ObjectId),
    ObjectIdentityCollision(ObjectId),
    RelationIdentityCollision(RelationId),
}

impl fmt::Display for StoreGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCustomKind => formatter.write_str("custom object kind must not be empty"),
            Self::EmptyCustomRelation => {
                formatter.write_str("custom relation kind must not be empty")
            }
            Self::EmptyNamespace => formatter.write_str("relation namespace must not be empty"),
            Self::MissingObject(id) => write!(formatter, "relation endpoint `{id}` is not stored"),
            Self::ObjectIdentityCollision(id) => {
                write!(formatter, "ObjectID collision for `{id}`")
            }
            Self::RelationIdentityCollision(id) => {
                write!(formatter, "RelationID collision for `{id}`")
            }
        }
    }
}

impl std::error::Error for StoreGraphError {}

#[derive(Clone, Debug, Default)]
pub struct ObjectGraph {
    objects: BTreeMap<ObjectId, LibraryObject>,
    relations: BTreeMap<RelationId, Relation>,
}

impl ObjectGraph {
    pub fn put(&mut self, draft: ObjectDraft) -> Result<ObjectId, StoreGraphError> {
        draft.kind.validate()?;
        let id = object_id(&draft);
        let object = LibraryObject {
            id: id.clone(),
            kind: draft.kind,
            meaning_id: draft.meaning_id,
            semantic_payload: draft.semantic_payload,
            presentation: draft.presentation,
        };
        if let Some(existing) = self.objects.get(&id) {
            if existing.kind != object.kind
                || existing.meaning_id != object.meaning_id
                || existing.semantic_payload != object.semantic_payload
            {
                return Err(StoreGraphError::ObjectIdentityCollision(id));
            }
            return Ok(existing.id.clone());
        }
        self.objects.insert(id.clone(), object);
        Ok(id)
    }

    pub fn add_relation(&mut self, draft: RelationDraft) -> Result<RelationId, StoreGraphError> {
        draft.kind.validate()?;
        if matches!(&draft.scope, RelationScope::Namespace(name) if name.trim().is_empty()) {
            return Err(StoreGraphError::EmptyNamespace);
        }
        for endpoint in [&draft.source, &draft.target] {
            if !self.objects.contains_key(endpoint) {
                return Err(StoreGraphError::MissingObject(endpoint.clone()));
            }
        }
        if let RelationScope::Object(scope) = &draft.scope {
            if !self.objects.contains_key(scope) {
                return Err(StoreGraphError::MissingObject(scope.clone()));
            }
        }

        let mut assumptions = draft.assumptions;
        assumptions.sort();
        assumptions.dedup();
        let mut evidence = draft.evidence;
        evidence.sort();
        evidence.dedup();
        let id = relation_id(
            &draft.kind,
            &draft.source,
            &draft.target,
            &draft.scope,
            &assumptions,
            draft.authority.as_deref(),
            &evidence,
        );
        let relation = Relation {
            id: id.clone(),
            kind: draft.kind,
            source: draft.source,
            target: draft.target,
            scope: draft.scope,
            assumptions,
            authority: draft.authority,
            evidence,
        };
        if let Some(existing) = self.relations.get(&id) {
            if existing != &relation {
                return Err(StoreGraphError::RelationIdentityCollision(id));
            }
            return Ok(existing.id.clone());
        }
        self.relations.insert(id.clone(), relation);
        Ok(id)
    }

    #[must_use]
    pub fn object(&self, id: &ObjectId) -> Option<&LibraryObject> {
        self.objects.get(id)
    }

    #[must_use]
    pub fn relation(&self, id: &RelationId) -> Option<&Relation> {
        self.relations.get(id)
    }

    pub fn objects(&self) -> impl Iterator<Item = &LibraryObject> {
        self.objects.values()
    }

    pub fn relations(&self) -> impl Iterator<Item = &Relation> {
        self.relations.values()
    }
}

fn object_id(draft: &ObjectDraft) -> ObjectId {
    let mut bytes = Vec::new();
    frame(&mut bytes, OBJECT_SCHEMA_V1.as_bytes());
    frame(&mut bytes, draft.kind.canonical_name().as_bytes());
    frame(&mut bytes, draft.meaning_id.as_str().as_bytes());
    frame(&mut bytes, &draft.semantic_payload);
    ObjectId::from_bytes(&bytes)
}

fn relation_id(
    kind: &RelationKind,
    source: &ObjectId,
    target: &ObjectId,
    scope: &RelationScope,
    assumptions: &[MeaningId],
    authority: Option<&str>,
    evidence: &[EvidenceId],
) -> RelationId {
    let mut bytes = Vec::new();
    frame(&mut bytes, RELATION_SCHEMA_V1.as_bytes());
    frame(&mut bytes, kind.canonical_name().as_bytes());
    frame(&mut bytes, source.as_str().as_bytes());
    frame(&mut bytes, target.as_str().as_bytes());
    match scope {
        RelationScope::Global => frame(&mut bytes, b"global"),
        RelationScope::Namespace(name) => {
            frame(&mut bytes, b"namespace");
            frame(&mut bytes, name.as_bytes());
        }
        RelationScope::Object(id) => {
            frame(&mut bytes, b"object");
            frame(&mut bytes, id.as_str().as_bytes());
        }
    }
    frame_many(
        &mut bytes,
        assumptions.iter().map(|id| id.as_str().as_bytes()),
    );
    match authority {
        Some(authority) => {
            bytes.push(1);
            frame(&mut bytes, authority.as_bytes());
        }
        None => bytes.push(0),
    }
    frame_many(&mut bytes, evidence.iter().map(|id| id.as_str().as_bytes()));
    RelationId::from_bytes(&bytes)
}

pub(crate) fn frame(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    bytes.extend_from_slice(value);
}

pub(crate) fn frame_many<'a>(bytes: &mut Vec<u8>, values: impl Iterator<Item = &'a [u8]>) {
    let values = values.collect::<Vec<_>>();
    bytes.extend_from_slice(
        &u64::try_from(values.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for value in values {
        frame(bytes, value);
    }
}
