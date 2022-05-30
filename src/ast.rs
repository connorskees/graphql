use std::{collections::HashMap, fs};

use lasso::{Rodeo, Spur};

use crate::error::GraphqlParseError;

#[derive(Debug, Clone)]
pub struct Document {
    pub(crate) operations: HashMap<(Option<Spur>, OperationKind), Operation>,
    pub(crate) fragments: HashMap<Spur, Fragment>,
    pub(crate) input_objects: HashMap<Spur, InputObject>,
    pub(crate) output_objects: HashMap<Spur, ObjectType>,
    pub(crate) interfaces: HashMap<Spur, Interface>,
    pub(crate) scalars: HashMap<Spur, Scalar>,
    pub(crate) unions: HashMap<Spur, Union>,
    pub(crate) enums: HashMap<Spur, Enum>,
}

pub enum GraphqlSchemaTypeError {
    InterfaceDne(Spur),
}

impl Document {
    pub fn new() -> Self {
        Self {
            operations: HashMap::new(),
            fragments: HashMap::new(),
            input_objects: HashMap::new(),
            output_objects: HashMap::new(),
            interfaces: HashMap::new(),
            scalars: HashMap::new(),
            unions: HashMap::new(),
            enums: HashMap::new(),
        }
    }

    pub fn validate(&self) -> Vec<GraphqlSchemaTypeError> {
        let mut errors = Vec::new();

        for obj in self.output_objects.values() {
            for interface_name in &obj.implements {
                let interface = match self.interfaces.get(&interface_name.0) {
                    Some(i) => i,
                    None => {
                        errors.push(GraphqlSchemaTypeError::InterfaceDne(interface_name.0));
                        continue;
                    }
                };

                errors.append(&mut obj.validate_implements_fields(&interface.fields));
            }
        }

        errors
    }
}

#[derive(Debug, Clone)]
pub enum Definition {
    Operation(Operation),
    Fragment(Fragment),
    TypeDecl(TypeDefinition),
    TypeExtension,
}

#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) ty: Type,
    pub(crate) arguments: Option<Vec<InputObjectField>>,
    pub(crate) directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub(crate) name: Spur,
    pub(crate) value: Value,
}

#[derive(Debug, Clone)]
pub enum Type {
    Named { name: Spur, nullable: bool },
    List { base: Box<Self>, nullable: bool },
}

impl Type {
    pub fn set_nonnullable(&mut self) {
        match self {
            Self::Named { nullable, .. } | Self::List { nullable, .. } => *nullable = true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectType {
    pub(crate) implements: Vec<NamedType>,
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
    pub(crate) fields: Option<Vec<FieldDefinition>>,
}

impl ObjectType {
    pub fn validate_implements_fields(
        &self,
        fields: &[FieldDefinition],
    ) -> Vec<GraphqlSchemaTypeError> {
        let mut errors = Vec::new();

        errors
    }
}

#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Scalar(Scalar),
    Object(ObjectType),
    Interface(Interface),
    Union(Union),
    Enum(Enum),
    InputObject(InputObject),
}

#[derive(Debug, Clone)]
pub struct InputObjectField {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) ty: Type,
    pub(crate) default: Option<Value>,
    pub(crate) directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub enum Value {
    True,
    False,
    Null,
    String(Spur),
    Variable(Spur),
    EnumVariant(Spur),
    List(Vec<Self>),
    Object(HashMap<Spur, Self>),
    // todo: numbers
    Float,
    Int,
}

#[derive(Debug, Clone)]
pub struct InputObject {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
    pub(crate) fields: Option<Vec<InputObjectField>>,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
    pub(crate) variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub struct Directive {
    pub(crate) name: Spur,
    pub(crate) arguments: Option<Vec<Argument>>,
}

#[derive(Debug, Clone)]
pub struct NamedType(pub(crate) Spur);

#[derive(Debug, Clone)]
pub struct Union {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) types: Vec<NamedType>,
    pub(crate) directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub struct Scalar {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub struct Interface {
    pub(crate) description: Option<Spur>,
    pub(crate) name: Spur,
    pub(crate) directives: Vec<Directive>,
    pub(crate) fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone)]
pub struct Fragment {
    pub(crate) name: Spur,
    pub(crate) on: Spur,
    pub(crate) directives: Vec<Directive>,
    pub(crate) selection_set: Vec<Selection>,
}

#[derive(Debug, Clone)]
pub enum Selection {
    Field {
        alias: Option<Spur>,
        name: Spur,
        arguments: Option<Vec<Argument>>,
        directives: Vec<Directive>,
        selection_set: Option<Vec<Self>>,
    },
    FragmentSpread {
        name: Spur,
        directives: Vec<Directive>,
    },
    InlineFragment {
        on: Spur,
        directives: Vec<Directive>,
        selection_set: Vec<Self>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Type,
    Input,
    Enum,
    Implements,
    Scalar,
    True,
    False,
    Union,
    Fragment,
    Query,
    Mutation,
    Subscription,
    Extend,
    Null,
    Interface,
    On,
}

impl Keyword {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Input => "input",
            Self::Enum => "enum",
            Self::Implements => "implements",
            Self::Scalar => "scalar",
            Self::True => "true",
            Self::False => "false",
            Self::Union => "union",
            Self::Fragment => "fragment",
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
            Self::Extend => "extend",
            Self::Null => "null",
            Self::Interface => "interface",
            Self::On => "on",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Name(Spur),
    String(Spur),
    Keyword(Keyword),

    // https://spec.graphql.org/June2018/#Punctuator
    Bang,
    Dollar,
    OpenParen,
    CloseParen,
    DotDotDot,
    Colon,
    Eq,
    AtSign,
    OpenSquareBrace,
    CloseSquareBrace,
    OpenCurlyBrace,
    Pipe,
    CloseCurlyBrace,

    Ampersand,

    IntValue,
    FloatValue,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub(crate) kind: OperationKind,
    pub(crate) name: Option<Spur>,
    pub(crate) variable_definitions: Vec<VariableDefinition>,
    pub(crate) directives: Vec<Directive>,
    pub(crate) selection_set: Vec<Selection>,
}

#[derive(Debug, Clone)]
pub struct VariableDefinition {
    pub(crate) name: Spur,
    pub(crate) ty: Type,
    pub(crate) default: Option<Value>,
}
