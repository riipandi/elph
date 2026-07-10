//! Skill load result and diagnostic types.

use crate::harness::types::Skill;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
    InvalidMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    pub code: SkillDiagnosticCode,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSkillsResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkill<TSkill, TSource> {
    pub skill: TSkill,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkillDiagnostic<TSource> {
    pub code: SkillDiagnosticCode,
    pub message: String,
    pub path: String,
    pub source: TSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSourcedSkillsResult<TSkill, TSource> {
    pub skills: Vec<SourcedSkill<TSkill, TSource>>,
    pub diagnostics: Vec<SourcedSkillDiagnostic<TSource>>,
}
