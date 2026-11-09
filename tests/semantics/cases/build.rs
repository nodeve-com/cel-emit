//! The Rust a case becomes.

use super::*;

/// The emitter that writes the cases, and the messages they build.
pub(super) struct Build {
    pub(super) messages: BTreeMap<String, Type>,
    pub(super) bodies: Vec<Body>,
    pub(super) em: Emitter,
}

impl Build {
    /// Declares the written functions to the emitter.
    pub(super) fn define(&mut self) {
        self.em.define(&self.bodies);
    }

    /// The Rust of case `n`: a function `c{n}` writing what each message gives, joined by
    /// [`SEP`].
    pub(super) fn function(&mut self, n: usize, case: &Case, types: &[(String, Type)]) -> String {
        let source = &case.source;
        let tree = self.lower(case, types);
        let bound = Bound((0..types.len()).map(|i| format!("i{i}.clone()")).collect());
        let expr = (self.em.expr(&tree, &bound)).unwrap_or_else(|e| panic!("{source}: {e}"));
        let rust_ty = self.em.rust(&tree.ty).unwrap();
        let shown = show(&tree.ty, "v", 0);
        let params: Vec<String> = types
            .iter()
            .enumerate()
            .map(|(i, (_, t))| format!("i{i}: {}", self.em.rust(t).unwrap()))
            .collect();
        let mut body = format!(
            "    let f = |{}| -> s::Result<String> {{ let v: {rust_ty} = {expr}; s::ok({shown}) }};\n    let mut out: Vec<String> = Vec::new();\n",
            params.join(", ")
        );
        let err = match case.want {
            Want::Exactly(Got::Status(_)) => "format!(\"err {e:?}\")",
            _ => "{ let _ = e; String::from(\"err\") }",
        };
        for message in &case.messages {
            let args = self.arguments(case, types, message);
            body.push_str(&format!(
                "    out.push(match (|| f({args}))() {{ Ok(t) => t, Err(s::Error::Absent) => String::from(\"absent\"), Err(e) => {err} }});\n"
            ));
        }
        format!("fn c{n}() -> String {{\n{body}    out.join(\"{SEP}\")\n}}\n")
    }

    /// `case` lowered over its bindings `types` and the messages.
    fn lower(&self, case: &Case, types: &[(String, Type)]) -> cel_emit::Typed {
        let mut decls = Decls {
            skip_failed: case.skip_failed,
            ..Decls::default()
        };
        decls.messages.clone_from(&self.messages);
        decls.define(&self.bodies);
        for (i, (name, ty)) in types.iter().enumerate() {
            decls.names.insert(name.clone(), Name::var(ty.clone(), i));
        }
        let source = &case.source;
        let lowered = match &case.result {
            Some(ty) => cel_emit::lower_as(source, &decls, ty),
            None => cel_emit::lower(source, &decls),
        };
        lowered.unwrap_or_else(|e| panic!("{source}: {e}"))
    }

    /// The Rust arguments `message` gives a case's function, one per binding.
    fn arguments(
        &mut self,
        case: &Case,
        types: &[(String, Type)],
        message: &[(String, Value)],
    ) -> String {
        let args: Vec<String> = types
            .iter()
            .zip(message)
            .map(|((_, t), (_, v))| {
                assert!(fits(v, t), "{}: {v:?} is no {t:?}", case.source);
                rust(v, t, &mut self.em)
            })
            .collect();
        args.join(", ")
    }
}
