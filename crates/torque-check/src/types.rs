// Copyright 2026 Riya Amemiya.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use torque_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    pub ty: TypeId,
    pub span: Span,
    pub indexed: bool,
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    Error,
    Void,
    Never,
    IntegerLiteral,
    StringLiteral,
    Abstract {
        name: String,
        parent: Option<TypeId>,
        is_constexpr: bool,
    },
    Alias {
        name: String,
        target: TypeId,
    },
    Union {
        members: Vec<TypeId>,
    },
    Class {
        name: String,
        parent: Option<TypeId>,
        fields: Vec<FieldInfo>,
    },
    Struct {
        name: String,
        parent: Option<TypeId>,
        fields: Vec<FieldInfo>,
        generic_params: Vec<String>,
    },
    Function {
        params: Vec<TypeId>,
        result: TypeId,
    },
    Enum {
        name: String,
        parent: Option<TypeId>,
        entries: Vec<(String, Span)>,
    },
    GenericParam {
        name: String,
    },
    Applied {
        name: String,
        args: Vec<TypeId>,
    },
}

#[derive(Clone, Debug)]
pub struct TypeData {
    pub kind: TypeKind,
    pub span: Span,
}

pub struct TypeStore {
    types: Vec<TypeData>,
}

impl TypeStore {
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    pub fn intern(&mut self, kind: TypeKind, span: Span) -> TypeId {
        if let TypeKind::Union { members } = &kind {
            let mut members = members.clone();
            members.sort_by_key(|id| id.0);
            members.dedup();
            for (index, existing) in self.types.iter().enumerate() {
                if let TypeKind::Union {
                    members: existing_members,
                } = &existing.kind
                    && *existing_members == members
                {
                    return TypeId(index as u32);
                }
            }
            self.types.push(TypeData {
                kind: TypeKind::Union { members },
                span,
            });
            return TypeId((self.types.len() - 1) as u32);
        }
        if let TypeKind::Function { params, result } = &kind {
            for (index, existing) in self.types.iter().enumerate() {
                if let TypeKind::Function {
                    params: existing_params,
                    result: existing_result,
                } = &existing.kind
                    && existing_params == params
                    && existing_result == result
                {
                    return TypeId(index as u32);
                }
            }
        }
        if let TypeKind::Applied { name, args } = &kind {
            for (index, existing) in self.types.iter().enumerate() {
                if let TypeKind::Applied {
                    name: existing_name,
                    args: existing_args,
                } = &existing.kind
                    && existing_name == name
                    && existing_args == args
                {
                    return TypeId(index as u32);
                }
            }
        }
        self.types.push(TypeData { kind, span });
        TypeId((self.types.len() - 1) as u32)
    }

    pub fn intern_applied(&mut self, name: String, args: Vec<TypeId>, span: Span) -> TypeId {
        self.intern(TypeKind::Applied { name, args }, span)
    }

    pub fn intern_generic_param(&mut self, name: String, span: Span) -> TypeId {
        for (index, existing) in self.types.iter().enumerate() {
            if let TypeKind::GenericParam {
                name: existing_name,
            } = &existing.kind
                && existing_name == &name
                && existing.span == span
            {
                return TypeId(index as u32);
            }
        }
        self.intern(TypeKind::GenericParam { name }, span)
    }

    pub fn is_constexpr(&self, id: TypeId) -> bool {
        match &self.get(self.unwrap_alias(id)).kind {
            TypeKind::Abstract { is_constexpr, .. } => *is_constexpr,
            TypeKind::IntegerLiteral | TypeKind::StringLiteral => true,
            _ => false,
        }
    }

    pub fn base_name(&self, id: TypeId) -> String {
        let name = self.name_of(id);
        name.strip_prefix("constexpr ").unwrap_or(&name).to_string()
    }

    pub fn get(&self, id: TypeId) -> &TypeData {
        &self.types[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: TypeId) -> &mut TypeData {
        &mut self.types[id.0 as usize]
    }

    pub fn unwrap_alias(&self, mut id: TypeId) -> TypeId {
        for _ in 0..16 {
            match &self.get(id).kind {
                TypeKind::Alias { target, .. } => id = *target,
                _ => return id,
            }
        }
        id
    }

    pub fn name_of(&self, id: TypeId) -> String {
        match &self.get(id).kind {
            TypeKind::Error => "<error>".into(),
            TypeKind::Void => "void".into(),
            TypeKind::Never => "never".into(),
            TypeKind::IntegerLiteral => "constexpr IntegerLiteral".into(),
            TypeKind::StringLiteral => "constexpr string".into(),
            TypeKind::Abstract {
                name, is_constexpr, ..
            } => {
                if *is_constexpr {
                    format!("constexpr {name}")
                } else {
                    name.clone()
                }
            }
            TypeKind::Alias { name, .. }
            | TypeKind::Class { name, .. }
            | TypeKind::Struct { name, .. }
            | TypeKind::Enum { name, .. }
            | TypeKind::GenericParam { name } => name.clone(),
            TypeKind::Function { params, result } => {
                let inner = params
                    .iter()
                    .map(|id| self.name_of(*id))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("builtin({}) => {}", inner, self.name_of(*result))
            }
            TypeKind::Union { members } => members
                .iter()
                .map(|id| self.name_of(*id))
                .collect::<Vec<_>>()
                .join(" | "),
            TypeKind::Applied { name, args } => {
                let inner = args
                    .iter()
                    .map(|id| self.name_of(*id))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}<{inner}>")
            }
        }
    }

    pub fn is_error(&self, id: TypeId) -> bool {
        matches!(self.get(self.unwrap_alias(id)).kind, TypeKind::Error)
    }

    pub fn is_never(&self, id: TypeId) -> bool {
        matches!(self.get(self.unwrap_alias(id)).kind, TypeKind::Never)
    }

    pub fn is_void(&self, id: TypeId) -> bool {
        matches!(self.get(self.unwrap_alias(id)).kind, TypeKind::Void)
    }

    pub fn is_integer_literal(&self, id: TypeId) -> bool {
        matches!(
            self.get(self.unwrap_alias(id)).kind,
            TypeKind::IntegerLiteral
        )
    }

    pub fn is_string_literal(&self, id: TypeId) -> bool {
        matches!(
            self.get(self.unwrap_alias(id)).kind,
            TypeKind::StringLiteral
        )
    }

    pub fn is_string_like(&self, id: TypeId) -> bool {
        let name = self.name_of(id);
        name == "string" || name == "constexpr string"
    }

    pub fn generic_param_name(&self, id: TypeId) -> Option<&str> {
        match &self.get(self.unwrap_alias(id)).kind {
            TypeKind::GenericParam { name } => Some(name.as_str()),
            _ => None,
        }
    }

    pub fn mentions_generic(&self, id: TypeId, name: &str) -> bool {
        let id = self.unwrap_alias(id);
        match &self.get(id).kind {
            TypeKind::GenericParam { name: found } => found == name,
            TypeKind::Applied { args, .. } => args
                .clone()
                .into_iter()
                .any(|arg| self.mentions_generic(arg, name)),
            TypeKind::Function { params, result } => {
                params
                    .clone()
                    .into_iter()
                    .any(|param| self.mentions_generic(param, name))
                    || self.mentions_generic(*result, name)
            }
            TypeKind::Union { members } => members
                .clone()
                .into_iter()
                .any(|member| self.mentions_generic(member, name)),
            _ => false,
        }
    }

    pub fn is_bool_like(&self, id: TypeId) -> bool {
        let name = self.name_of(id);
        name == "bool" || name == "constexpr bool" || name.ends_with("bool")
    }

    pub fn union_of(&mut self, left: TypeId, right: TypeId, span: Span) -> TypeId {
        if self.is_never(left) {
            return right;
        }
        if self.is_never(right) {
            return left;
        }
        if left == right {
            return left;
        }
        let mut members = Vec::new();
        self.collect_union(left, &mut members);
        self.collect_union(right, &mut members);
        members.sort_by_key(|id| id.0);
        members.dedup();
        if members.len() == 1 {
            return members[0];
        }
        self.intern(TypeKind::Union { members }, span)
    }

    fn collect_union(&self, id: TypeId, out: &mut Vec<TypeId>) {
        let mut seen = Vec::new();
        self.collect_union_rec(id, out, &mut seen);
    }

    fn collect_union_rec(&self, id: TypeId, out: &mut Vec<TypeId>, seen: &mut Vec<TypeId>) {
        let id = self.unwrap_alias(id);
        if seen.contains(&id) {
            return;
        }
        seen.push(id);
        match &self.get(id).kind {
            TypeKind::Union { members } => {
                for member in members {
                    self.collect_union_rec(*member, out, seen);
                }
            }
            _ => out.push(id),
        }
    }

    pub fn is_subtype(&self, sub: TypeId, sup: TypeId) -> bool {
        let mut seen = Vec::new();
        self.is_subtype_rec(sub, sup, &mut seen)
    }

    fn is_subtype_rec(&self, sub: TypeId, sup: TypeId, seen: &mut Vec<(TypeId, TypeId)>) -> bool {
        let sub = self.unwrap_alias(sub);
        let sup = self.unwrap_alias(sup);
        if sub == sup || self.is_never(sub) || self.is_error(sub) || self.is_error(sup) {
            return true;
        }
        let pair = (sub, sup);
        if seen.contains(&pair) {
            return false;
        }
        seen.push(pair);
        if let TypeKind::Union { members } = &self.get(sub).kind {
            let members = members.clone();
            return members
                .iter()
                .all(|member| self.is_subtype_rec(*member, sup, seen));
        }
        if let TypeKind::Union { members } = &self.get(sup).kind {
            let members = members.clone();
            return members
                .iter()
                .any(|member| self.is_subtype_rec(sub, *member, seen));
        }
        if let TypeKind::Applied {
            name: sub_name,
            args: sub_args,
        } = &self.get(sub).kind
            && let TypeKind::Applied {
                name: sup_name,
                args: sup_args,
            } = &self.get(sup).kind
        {
            if sub_name != sup_name || sub_args.len() != sup_args.len() {
                return false;
            }
            let sub_args = sub_args.clone();
            let sup_args = sup_args.clone();
            return sub_args
                .iter()
                .zip(sup_args.iter())
                .all(|(left, right)| self.is_subtype_rec(*left, *right, seen));
        }
        let mut current = Some(sub);
        let mut walked = 0;
        while let Some(id) = current {
            if id == sup {
                return true;
            }
            current = match &self.get(id).kind {
                TypeKind::Abstract { parent, .. }
                | TypeKind::Class { parent, .. }
                | TypeKind::Struct { parent, .. }
                | TypeKind::Enum { parent, .. } => *parent,
                _ => None,
            };
            walked += 1;
            if walked > 32 {
                break;
            }
        }
        false
    }

    pub fn fields_of(&self, id: TypeId) -> Vec<FieldInfo> {
        let mut seen = Vec::new();
        self.fields_of_rec(id, &mut seen)
    }

    fn fields_of_rec(&self, id: TypeId, seen: &mut Vec<TypeId>) -> Vec<FieldInfo> {
        let id = self.unwrap_alias(id);
        if seen.contains(&id) {
            return Vec::new();
        }
        seen.push(id);
        match &self.get(id).kind {
            TypeKind::Class { fields, parent, .. } => {
                let parent = *parent;
                let fields = fields.clone();
                let mut all = parent
                    .map(|p| self.fields_of_rec(p, seen))
                    .unwrap_or_default();
                all.extend(fields);
                all
            }
            TypeKind::Struct { fields, parent, .. } => {
                let parent = *parent;
                let fields = fields.clone();
                let mut all = parent
                    .map(|p| self.fields_of_rec(p, seen))
                    .unwrap_or_default();
                all.extend(fields);
                all
            }
            TypeKind::Union { members } => {
                let members = members.clone();
                let mut nonempty = Vec::new();
                for member in members {
                    let mut member_seen = seen.clone();
                    let fields = self.fields_of_rec(member, &mut member_seen);
                    if !fields.is_empty() {
                        nonempty.push(fields);
                    }
                }
                let Some((first, rest)) = nonempty.split_first() else {
                    return Vec::new();
                };
                let mut common = first.clone();
                for fields in rest {
                    common.retain(|field| fields.iter().any(|other| other.name == field.name));
                }
                common
            }
            TypeKind::Abstract { parent, .. } => parent
                .map(|p| self.fields_of_rec(p, seen))
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    pub fn numeric_like(&self, id: TypeId) -> bool {
        if self.is_integer_literal(id) {
            return true;
        }
        let id = self.unwrap_alias(id);
        if let TypeKind::Applied { .. } = &self.get(id).kind {
            return false;
        }
        let name = self.base_name(id);
        const NAMES: &[&str] = &[
            "Smi",
            "Number",
            "Numeric",
            "int31",
            "int32",
            "uint32",
            "intptr",
            "uintptr",
            "float64",
            "float32",
            "bint",
            "int64",
            "uint64",
            "IntegerLiteral",
        ];
        if NAMES.iter().any(|n| name == *n)
            || name.starts_with("int")
            || name.starts_with("uint")
            || name.starts_with("float")
            || name.starts_with("char")
            || name.ends_with("Smi")
            || name.ends_with("Number")
            || name.contains("Integer")
        {
            return true;
        }
        match &self.get(id).kind {
            TypeKind::Abstract {
                parent: Some(parent),
                ..
            }
            | TypeKind::Class {
                parent: Some(parent),
                ..
            }
            | TypeKind::Struct {
                parent: Some(parent),
                ..
            }
            | TypeKind::Enum {
                parent: Some(parent),
                ..
            } if *parent != id => self.numeric_like(*parent),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldInfo, TypeKind, TypeStore};
    use torque_span::Span;

    #[test]
    fn fields_of_survives_object_alias_to_child_union() {
        let mut types = TypeStore::new();
        let dummy = Span::dummy();
        let object = types.intern(
            TypeKind::Abstract {
                name: "Object".into(),
                parent: None,
                is_constexpr: false,
            },
            dummy,
        );
        let smi = types.intern(
            TypeKind::Abstract {
                name: "Smi".into(),
                parent: Some(object),
                is_constexpr: false,
            },
            dummy,
        );
        let heap = types.intern(
            TypeKind::Class {
                name: "HeapObject".into(),
                parent: Some(object),
                fields: vec![FieldInfo {
                    name: "map".into(),
                    ty: object,
                    span: dummy,
                    indexed: false,
                }],
            },
            dummy,
        );
        let union = types.union_of(smi, heap, dummy);
        types.get_mut(object).kind = TypeKind::Alias {
            name: "Object".into(),
            target: union,
        };
        let fields = types.fields_of(heap);
        assert!(fields.iter().any(|field| field.name == "map"), "{fields:?}");
        assert!(types.is_subtype(heap, object));
    }
}
