//! 3-part pedagogic CLI error formatting (WHAT, WHERE, REMEDIATION).
//!
//! Provides structured human-readable stderr diagnostic and machine-readable JSON
//! stdout envelope conforming to the Agent Ergonomics CLI specification.

use crate::CliExit;
use emath_artifact::JsonWriter;

#[derive(Clone, Debug)]
pub struct PedagogicError {
    pub code: &'static str,
    pub what: String,
    pub where_context: String,
    pub remediation: String,
    pub command: Option<String>,
    pub flag: Option<String>,
    pub usage: Option<String>,
    pub help_cmd: Option<String>,
    pub did_you_mean: Option<String>,
}

impl PedagogicError {
    pub fn new(
        code: &'static str,
        what: impl Into<String>,
        where_context: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            code,
            what: what.into(),
            where_context: where_context.into(),
            remediation: remediation.into(),
            command: None,
            flag: None,
            usage: None,
            help_cmd: None,
            did_you_mean: None,
        }
    }

    pub fn with_command(mut self, cmd: impl Into<String>) -> Self {
        let c = cmd.into();
        if self.help_cmd.is_none() {
            self.help_cmd = Some(format!("emath help {c}"));
        }
        self.command = Some(c);
        self
    }

    pub fn with_flag(mut self, flag: impl Into<String>) -> Self {
        self.flag = Some(flag.into());
        self
    }

    pub fn with_usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = Some(usage.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help_cmd = Some(help.into());
        self
    }

    pub fn with_did_you_mean(mut self, hint: impl Into<String>) -> Self {
        self.did_you_mean = Some(hint.into());
        self
    }

    pub fn emit(&self, json: bool) -> CliExit {
        if json {
            let mut obj = JsonWriter::object();
            obj.string("status", "error");
            obj.string("code", self.code);
            obj.string("what", &self.what);
            obj.string("where", &self.where_context);
            obj.string("remediation", &self.remediation);
            obj.string("message", &self.what);
            if let Some(cmd) = &self.command {
                obj.string("command", cmd);
            }
            if let Some(flag) = &self.flag {
                obj.string("flag", flag);
            }
            if let Some(hint) = &self.did_you_mean {
                obj.string("did_you_mean", hint);
            }
            if let Some(usage) = &self.usage {
                obj.string("usage", usage);
            }
            if let Some(help) = &self.help_cmd {
                obj.string("try", help);
            }
            println!("{}", obj.finish());
        } else {
            eprintln!("error [{}]: {}", self.code, self.what);
            eprintln!("  where:       {}", self.where_context);
            if let Some(hint) = &self.did_you_mean {
                eprintln!("  did you mean: `{hint}`");
            }
            eprintln!("  remediation: {}", self.remediation);
            if let Some(usage) = &self.usage {
                eprintln!("  usage:       {}", usage);
            }
            if let Some(help) = &self.help_cmd {
                eprintln!("  try:         {}", help);
            }
        }
        CliExit::Usage
    }
}
