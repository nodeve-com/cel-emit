//! Names: declared, constant, a comprehension's, a message's field by its address, or a map of
//! messages' field.

use super::parts::is_fields;
use super::scope::Scoped;
use super::*;

impl<'a> Lower<'a> {
    /// A declared name, dotted or not; a constant; a comprehension's element, index, key, or
    /// message field; a field of a map of messages' message; or a field of a message by its address.
    pub(crate) fn name(&mut self, e: &'a SpannedExpr) -> Result<Node, Error> {
        if let Some((map, key)) = self.message_field(e)? {
            return Ok(Node::Call("index_map".into(), vec![map, key]));
        }
        let path = self.path(e);
        let mut parts = path.iter().flat_map(|p| p.split('.'));
        let (root, field) = (parts.next(), parts.next());
        let by_bind = matches!(root.map(|r| self.local(r)), Some(Some(Scoped::Bind(_))));
        let looped = !by_bind && root.is_some();
        for (n, l) in self.loops.iter().enumerate().rev().filter(|_| looped) {
            if root == l.index.as_deref() && field.is_none() {
                return Ok(match &l.over {
                    Over::List(_) => Node::Position(n),
                    Over::Map(key, _) => Typed::stored(key.clone(), Node::Item(n, Part::Key)).node,
                    Over::Messages(_) => Node::Item(n, Part::Key),
                });
            }
            if root == l.item.as_deref() {
                let (ty, part) = match &l.over {
                    Over::List(ty) => (ty, Part::Whole),
                    Over::Map(_, ty) => (ty, Part::Value),
                    Over::Messages(_) => {
                        return unsupported("a map of messages' message other than by its fields")
                    }
                };
                let item = Typed::stored(ty.clone(), Node::Item(n, part));
                let rest: Vec<&str> = path.iter().flat_map(|p| p.split('.')).skip(1).collect();
                return self.fields(item, &rest);
            }
        }
        if let Some(p) = path.as_deref().filter(|_| !by_bind) {
            match self.found(p) {
                Some(At::Constant(c)) => return Ok(c.node),
                Some(At::Var(v)) => return Ok(self.declared(p, v).node),
                None => {}
            }
        }
        // A field of a message a literal or a bound name gives.
        if let Some(v) = self.selected_field(e)? {
            return match is_fields(&v) {
                false => Ok(self.read(e, v)),
                true => Ok(super::message::whole(v)?.node),
            };
        }
        // A field of a message a value gives: a list's element, say.
        if let Expr::Member { expr, field, .. } = &e.node {
            let base = self.expr(expr)?;
            match &base.ty {
                Type::Message(_) => return self.fields(base, &[field.as_str()]),
                // A string-keyed map's field is its entry: a key it lacks is no such key.
                Type::Map(k, _) if matches!(**k, Type::String | Type::Dyn) => {
                    let key = super::dynamic::field_key(k, field)?;
                    return Ok(Node::Call("index_map".into(), vec![base, key]));
                }
                Type::Dyn => {
                    let key = Typed::string(field);
                    return Ok(Node::Call("dyn_get".into(), vec![base, key]));
                }
                _ => {}
            }
        }
        unsupported("field access on a value")
    }

    /// `path` of fields into `message`, a message by its address.
    pub(crate) fn fields(&self, message: Typed, path: &[&str]) -> Result<Node, Error> {
        let Some((field, rest)) = path.split_first() else {
            return Ok(message.node);
        };
        if message.ty == Type::Dyn {
            let key = Typed::string(field);
            let read = Node::Call("dyn_get".into(), vec![message, key]);
            return self.fields(Typed::new(Type::Dyn, read), rest);
        }
        let fields = message.ty.fields();
        let i = fields
            .iter()
            .position(|f| f.name == *field)
            .ok_or_else(|| Error::Invalid(format!("no field {field}")))?;
        let ty = fields.get(i).map(|f| f.ty.clone()).unwrap_or(Type::Int);
        let field = Typed::stored(ty, Node::Field(Box::new(message), i));
        // An optional field read plainly is its value, or absent.
        if let (Type::Optional(_), true) = (&field.ty, rest.is_empty()) {
            return Ok(Node::Call("optional_value".into(), vec![field]));
        }
        self.fields(field, rest)
    }

    /// `e` as a field of a map of messages' message, `<map>[<key>].<field>` or a comprehension's
    /// message's: that field's map, and the key.
    pub(crate) fn message_field(
        &mut self,
        e: &'a SpannedExpr,
    ) -> Result<Option<(Typed, Typed)>, Error> {
        let mut fields = Vec::new();
        let mut base = e;
        while let Expr::Member { expr, field, .. } | Expr::MemberTestOnly { expr, field } =
            &base.node
        {
            fields.push(field.as_str());
            base = expr;
        }
        fields.reverse();
        let string = |node| Typed {
            ty: Type::String,
            node,
        };
        let (root, key) = match &base.node {
            Expr::Index {
                expr,
                index,
                optional: false,
            } => match &expr.node {
                Expr::Ident(root) if self.decls.columns.contains_key(root) => {
                    (root.clone(), self.expr(index)?)
                }
                _ => return Ok(None),
            },
            Expr::Ident(name) => {
                let bound = match self.local(name) {
                    Some(Scoped::Loop(n, l)) => match l {
                        Loop {
                            item,
                            over: Over::Messages(root),
                            ..
                        } if item.as_deref() == Some(name.as_str()) => Some((root.clone(), n)),
                        _ => None,
                    },
                    _ => None,
                };
                match bound {
                    Some((root, n)) => (root, string(Node::Item(n, Part::Key))),
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        if fields.is_empty() {
            return unsupported("a map of messages' message other than by its fields");
        }
        let map = self.column(&root, &fields.join("."))?;
        Ok(Some((map, key)))
    }

    /// A map of messages' `field` of each message, a map.
    pub(crate) fn column(&self, root: &str, field: &str) -> Result<Typed, Error> {
        let name = self
            .decls
            .names
            .get(&format!("{root}.{field}"))
            .ok_or_else(|| Error::Invalid(format!("{root} has no field {field}")))?;
        Ok(match &name.at {
            At::Var(v) => Typed::stored(name.ty.clone(), Node::Var(*v)),
            At::Constant(c) => c.clone(),
        })
    }

    /// The declared name `path`, variable `v`, read: widened where it is stored at a width.
    pub(crate) fn declared(&self, path: &str, v: usize) -> Typed {
        let ty = self.decls.names.get(path).map(|n| n.ty.clone());
        Typed::stored(ty.unwrap_or(Type::Int), Node::Var(v))
    }

    /// The field a map of messages loops over.
    pub(crate) fn loop_column(&self, root: &str) -> Result<String, Error> {
        self.decls
            .columns
            .get(root)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("{root} is no map of messages")))
    }

    /// `e` as a map of messages' name, if it is one.
    pub(crate) fn messages(&self, e: &SpannedExpr) -> Option<String> {
        match &e.node {
            Expr::Ident(name) if self.decls.columns.contains_key(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// `e` as a dotted name, `.?` read as `.`: `prev.?Smooth.watts` is `prev.Smooth.watts`.
    pub(crate) fn path(&self, e: &SpannedExpr) -> Option<String> {
        match &e.node {
            Expr::Ident(name) => Some(name.clone()),
            Expr::Member { expr, field, .. } | Expr::MemberTestOnly { expr, field } => {
                Some(format!("{}.{field}", self.path(expr)?))
            }
            _ => None,
        }
    }

    /// What a declared name is.
    pub(crate) fn found(&self, path: &str) -> Option<At> {
        self.decls.names.get(path).map(|n| n.at.clone())
    }
}
