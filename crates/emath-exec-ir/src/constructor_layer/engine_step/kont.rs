use super::super::*;
use super::prelude::{apply_unary, binary, cons_values, index_seq, type_admits};

impl Engine {
    pub(in crate::constructor_layer) fn snapshot(&self) -> Checkpoint {
        Checkpoint {
            schema: CHECKPOINT_SCHEMA.into(),
            source_id: self.source_id.clone(),
            image: IMAGE_IDENTITY.into(),
            abi: CHECKPOINT_ABI.into(),
            work: self.work,
            remaining: self.work_limit.saturating_sub(self.work),
            accounting: ACCOUNTING_VERSION.into(),
            memo: self.memo.clone(),
            frames: self.frames.clone(),
            scopes: self.scopes.clone(),
            function: self.entry.clone(),
            source: self.source_text.clone(),
            inputs: self.inputs.clone(),
            next_ref: self.next_ref,
        }
    }

    pub(in crate::constructor_layer) fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), ConstructorError> {
        if checkpoint.schema != CHECKPOINT_SCHEMA || checkpoint.abi != CHECKPOINT_ABI {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint schema or ABI does not match this constructor layer",
            ));
        }
        if !checkpoint.image.is_empty() && checkpoint.image != IMAGE_IDENTITY {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint image identity does not match this constructor layer",
            ));
        }
        if !checkpoint.source_id.is_empty()
            && !self.source_id.is_empty()
            && checkpoint.source_id != self.source_id
        {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint source identity does not match",
            ));
        }
        self.work = checkpoint.work;
        self.memo = checkpoint.memo.clone();
        self.visit = 0;
        self.next_ref = checkpoint.next_ref;
        self.scopes = checkpoint.scopes.clone();
        self.resume_frames = checkpoint.frames.clone();
        self.frames.clear();
        Ok(())
    }

    pub(in crate::constructor_layer) fn finish_from_stack(&mut self) -> Result<CValue, ConstructorError> {
        let frames = std::mem::take(&mut self.resume_frames);
        if frames.is_empty() {
            return Err(fault(
                "incompatible_checkpoint",
                "checkpoint has no remaining-work frames",
            ));
        }
        self.frames = frames;
        self.call_depth = u32::try_from(self.frames.len()).unwrap_or(u32::MAX);
        let innermost = self
            .frames
            .last()
            .cloned()
            .ok_or_else(|| fault("incompatible_checkpoint", "checkpoint frame stack is empty"))?;
        self.env = innermost.env.clone();
        let mut value = self.resume_current(&innermost)?;
        while !self.frames.is_empty() {
            self.pop_frame();
            self.call_depth = u32::try_from(self.frames.len()).unwrap_or(0);
            if self.frames.is_empty() {
                return Ok(value);
            }
            let parent = self.frames.last().cloned().ok_or_else(|| {
                fault("incompatible_checkpoint", "checkpoint frame stack is empty")
            })?;
            self.env = parent.env.clone();
            value = self.return_into_kont(value)?;
        }
        Ok(value)
    }

    pub(in crate::constructor_layer) fn resume_current(&mut self, frame: &ContinuationFrame) -> Result<CValue, ConstructorError> {
        if !frame.kont.is_empty() {
            return self.drain_kont_resume();
        }
        if let Some(decl) = self.functions.get(&frame.function).cloned() {
            let mut started = frame.next.is_empty();
            let mut last = CValue::Unit;
            for (name, expr) in &decl.defs {
                if !started {
                    if let Some(value) = frame.env.get(name) {
                        self.env.insert(name.clone(), value.clone());
                        last = value.clone();
                    }
                    if name == &frame.next {
                        started = true;
                        self.set_frame_next(name);
                        last = self.eval(expr)?;
                        self.env.insert(name.clone(), last.clone());
                    }
                    continue;
                }
                self.set_frame_next(name);
                last = self.eval(expr)?;
                self.env.insert(name.clone(), last.clone());
            }
            return self.function_result(&decl, &frame.function, last);
        }
        if let Some(kont) = frame.kont.first() {
            if let Kont::EvalExpr { expr } = kont {
                return self.eval(expr);
            }
        }
        Err(fault(
            "incompatible_checkpoint",
            format!("frame `{}` has no remaining-work instruction", frame.function),
        ))
    }

    pub(in crate::constructor_layer) fn drain_kont_resume(&mut self) -> Result<CValue, ConstructorError> {
        let Some(top) = self.pop_kont() else {
            return Err(fault(
                "incompatible_checkpoint",
                "remaining-work continuation is empty",
            ));
        };
        let value = match &top {
            Kont::BinLeft { op, left, right } => {
                let left_value = self.eval(left)?;
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: left_value.clone(),
                    right: right.clone(),
                });
                let right_value = self.eval(right)?;
                self.pop_kont();
                binary(*op, left_value, right_value)?
            }
            Kont::BinRight { op, left, right } => {
                let right_value = self.eval(right)?;
                binary(*op, left.clone(), right_value)?
            }
            Kont::IfAfterCond {
                condition,
                then_value,
                else_value,
            } => match self.eval(condition)? {
                CValue::Bool(true) => {
                    self.push_kont(Kont::IfThen {
                        then_value: then_value.clone(),
                    });
                    let value = self.eval(then_value)?;
                    self.pop_kont();
                    value
                }
                CValue::Bool(false) => {
                    self.push_kont(Kont::IfElse {
                        else_value: else_value.clone(),
                    });
                    let value = self.eval(else_value)?;
                    self.pop_kont();
                    value
                }
                _ => return Err(fault("type", "if condition must be Bool")),
            },
            Kont::IfThen { then_value } => self.eval(then_value)?,
            Kont::IfElse { else_value } => self.eval(else_value)?,
            Kont::CallArgs {
                callee,
                done,
                rest,
            } => self.finish_call_args(callee.clone(), done.clone(), rest.clone(), None)?,
            Kont::FnCall { name, done, rest } => {
                self.finish_fn_call(name.clone(), done.clone(), rest.clone(), None)?
            }
            Kont::SeqItems {
                as_tuple,
                done,
                rest,
            } => self.finish_seq_items(*as_tuple, done.clone(), rest.clone(), None)?,
            Kont::RecordFields {
                type_path,
                done,
                current,
                current_expr,
                rest,
            } => {
                let value = self.eval(current_expr)?;
                self.finish_record_fields(
                    type_path.clone(),
                    done.clone(),
                    rest.clone(),
                    Some((current.clone(), value)),
                )?
            }
            Kont::IndexAfterSeq { seq, index } => {
                let CValue::Int(i) = self.eval(index)? else {
                    return Err(fault("type", "index must be Int"));
                };
                index_seq(seq.clone(), i)?
            }
            Kont::UnaryAfter { op, value } => apply_unary(*op, self.eval(value)?)?,
            Kont::ConsLeft { head, tail } => {
                let h = self.eval(head)?;
                self.push_kont(Kont::ConsAfterHead {
                    head: h.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(h, t)?
            }
            Kont::ConsAfterHead { head, tail } => cons_values(head.clone(), self.eval(tail)?)?,
            Kont::MatchWaiting {
                subject,
                arms,
                else_arm,
            } => {
                let scrutinee = self.eval(subject)?;
                self.choose_match(scrutinee, arms, else_arm)?
            }
            Kont::CasesArm {
                cond,
                value,
                rest,
                else_arm,
            } => {
                let cond_value = self.eval(cond)?;
                if self.cases_cond_taken(&cond_value)? {
                    self.eval(value)?
                } else {
                    self.eval_cases_arms(rest, else_arm)?
                }
            }
            Kont::OpenAfterPacked {
                type_name,
                param,
                packed,
                body,
            } => {
                let value = self.eval(packed)?;
                self.finish_open(type_name, param, body, value)?
            }
            Kont::EvalExpr { expr } => self.eval(expr)?,
        };
        self.return_into_kont(value)
    }

    pub(in crate::constructor_layer) fn return_into_kont(&mut self, incoming: CValue) -> Result<CValue, ConstructorError> {
        let Some(top) = self.pop_kont() else {
            return self.finish_after_value(incoming);
        };
        let value = match &top {
            Kont::BinLeft { op, right, .. } => {
                self.push_kont(Kont::BinRight {
                    op: *op,
                    left: incoming.clone(),
                    right: right.clone(),
                });
                let right_value = self.eval(right)?;
                self.pop_kont();
                binary(*op, incoming, right_value)?
            }
            Kont::BinRight { op, left, .. } => binary(*op, left.clone(), incoming)?,
            Kont::IfAfterCond {
                then_value,
                else_value,
                ..
            } => match incoming {
                CValue::Bool(true) => {
                    self.push_kont(Kont::IfThen {
                        then_value: then_value.clone(),
                    });
                    let value = self.eval(then_value)?;
                    self.pop_kont();
                    value
                }
                CValue::Bool(false) => {
                    self.push_kont(Kont::IfElse {
                        else_value: else_value.clone(),
                    });
                    let value = self.eval(else_value)?;
                    self.pop_kont();
                    value
                }
                _ => return Err(fault("type", "if condition must be Bool")),
            },
            Kont::IfThen { .. } | Kont::IfElse { .. } | Kont::EvalExpr { .. } => incoming,
            Kont::CallArgs {
                callee,
                done,
                rest,
            } => self.finish_call_args(callee.clone(), done.clone(), rest.clone(), Some(incoming))?,
            Kont::FnCall { name, done, rest } => {
                self.finish_fn_call(name.clone(), done.clone(), rest.clone(), Some(incoming))?
            }
            Kont::SeqItems {
                as_tuple,
                done,
                rest,
            } => self.finish_seq_items(*as_tuple, done.clone(), rest.clone(), Some(incoming))?,
            Kont::RecordFields {
                type_path,
                done,
                current,
                rest,
                ..
            } => self.finish_record_fields(
                type_path.clone(),
                done.clone(),
                rest.clone(),
                Some((current.clone(), incoming)),
            )?,
            Kont::IndexAfterSeq { seq, .. } => match incoming {
                CValue::Int(i) => index_seq(seq.clone(), i)?,
                _ => return Err(fault("type", "index must be Int")),
            },
            Kont::UnaryAfter { op, .. } => apply_unary(*op, incoming)?,
            Kont::ConsLeft { tail, .. } => {
                self.push_kont(Kont::ConsAfterHead {
                    head: incoming.clone(),
                    tail: tail.clone(),
                });
                let t = self.eval(tail)?;
                self.pop_kont();
                cons_values(incoming, t)?
            }
            Kont::ConsAfterHead { head, .. } => cons_values(head.clone(), incoming)?,
            Kont::MatchWaiting { arms, else_arm, .. } => {
                self.choose_match(incoming, arms, else_arm)?
            }
            Kont::CasesArm {
                value,
                rest,
                else_arm,
                ..
            } => {
                if self.cases_cond_taken(&incoming)? {
                    self.eval(value)?
                } else {
                    self.eval_cases_arms(rest, else_arm)?
                }
            }
            Kont::OpenAfterPacked {
                type_name,
                param,
                body,
                ..
            } => self.finish_open(type_name, param, body, incoming)?,
        };
        self.return_into_kont(value)
    }

    pub(in crate::constructor_layer) fn finish_after_value(&mut self, incoming: CValue) -> Result<CValue, ConstructorError> {
        let Some(frame) = self.frames.last().cloned() else {
            return Ok(incoming);
        };
        let Some(decl) = self.functions.get(&frame.function).cloned() else {
            return Ok(incoming);
        };
        if frame.next == "__done" {
            return Ok(incoming);
        }
        let mut last = incoming;
        if !frame.next.is_empty() && frame.next != "body" {
            self.env.insert(frame.next.clone(), last.clone());
        }
        let mut seen = frame.next.is_empty() || frame.next == "body";
        for (name, expr) in &decl.defs {
            if !seen {
                if name == &frame.next {
                    seen = true;
                }
                continue;
            }
            self.set_frame_next(name);
            last = self.eval(expr)?;
            self.env.insert(name.clone(), last.clone());
        }
        self.function_result(&decl, &frame.function, last)
    }

    pub(in crate::constructor_layer) fn function_result(
        &self,
        decl: &FnDecl,
        name: &str,
        last: CValue,
    ) -> Result<CValue, ConstructorError> {
        if decl.outputs.len() > 1 {
            let mut fields = BTreeMap::new();
            for output in &decl.outputs {
                let value = self.env.get(output).cloned().ok_or_else(|| {
                    fault("unbound", format!("missing output `{output}`"))
                })?;
                fields.insert(output.clone(), value);
            }
            return Ok(CValue::Record {
                type_name: name.to_string(),
                fields: Arc::new(fields),
            });
        }
        if let Some(output) = &decl.output {
            let value = self
                .env
                .get(output)
                .cloned()
                .ok_or_else(|| fault("unbound", format!("missing output `{output}`")))?;
            if let Some(ty) = decl.output_types.first() {
                if !type_admits(ty, &value) {
                    return Err(fault(
                        "type",
                        format!("output `{output}` does not have the declared type, found {value}"),
                    ));
                }
            }
            return Ok(value);
        }
        Ok(last)
    }

}
