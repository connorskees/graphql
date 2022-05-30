use std::collections::HashMap;

use lasso::Spur;

use crate::{
    ast::{
        Argument, Directive, Document, Enum, EnumVariant, FieldDefinition, Fragment, InputObject,
        InputObjectField, Interface, Keyword, NamedType, ObjectType, Operation, OperationKind,
        Scalar, Selection, Token, Type, Union, Value, VariableDefinition,
    },
    error::GraphqlParseError,
    lexer::Lexer,
};

pub struct GraphqlParser<'a> {
    lexer: Lexer<'a>,
    document: Document,
}

impl<'a> GraphqlParser<'a> {
    pub fn parse(buffer: &'a [u8]) -> Result<Document, GraphqlParseError> {
        let mut parser = Self {
            lexer: Lexer::new(buffer),
            document: Document::new(),
        };

        loop {
            if !parser.next_definition()? {
                break;
            }
        }

        Ok(parser.document)
    }

    #[track_caller]
    fn expect_name(&mut self) -> Result<Spur, GraphqlParseError> {
        match self.lexer.next_token()? {
            Some(Token::Name(name)) => Ok(name),
            Some(Token::Keyword(keyword)) => {
                Ok(self.lexer.interner.get_or_intern(keyword.as_str()))
            }
            token => todo!("{:?}", token),
        }
    }

    fn expect_token(&mut self, token: Token) -> Result<(), GraphqlParseError> {
        let next = self.lexer.next_token()?;

        if Some(&token) == next.as_ref() {
            return Ok(());
        }

        Err(GraphqlParseError::ExpectedToken { token, found: next })
    }

    #[track_caller]
    fn parse_value(&mut self) -> Result<Value, GraphqlParseError> {
        Ok(match self.lexer.next_token()? {
            Some(Token::String(string)) => Value::String(string),
            Some(Token::Keyword(Keyword::True)) => Value::True,
            Some(Token::Keyword(Keyword::False)) => Value::False,
            Some(Token::Keyword(Keyword::Null)) => Value::Null,
            Some(Token::Dollar) => Value::Variable(self.expect_name()?),
            Some(Token::Name(name)) => Value::EnumVariant(name),
            Some(Token::OpenSquareBrace) => Value::List(self.parse_list_value()?),
            Some(Token::OpenCurlyBrace) => Value::Object(self.parse_object_value()?),
            token => todo!("{:?}", token),
        })
    }

    fn parse_list_value(&mut self) -> Result<Vec<Value>, GraphqlParseError> {
        let mut values = Vec::new();

        while !self.lexer.consume_byte_if_eq(b']') {
            values.push(self.parse_value()?);
        }

        Ok(values)
    }

    fn parse_object_value(&mut self) -> Result<HashMap<Spur, Value>, GraphqlParseError> {
        let mut values = HashMap::new();

        while !self.lexer.consume_byte_if_eq(b'}') {
            let key = self.expect_name()?;

            self.lexer.expect_byte(b':')?;

            let value = self.parse_value()?;

            values.insert(key, value);
        }

        Ok(values)
    }

    fn consume_token_if_eq(&mut self, token: Token) -> Result<bool, GraphqlParseError> {
        let next = self.lexer.peek_token()?;

        Ok(if Some(token) == next {
            self.lexer.next_token()?;

            true
        } else {
            false
        })
    }

    pub(crate) fn next_definition(&mut self) -> Result<bool, GraphqlParseError> {
        let description = self.parse_optional_description()?;

        match self.lexer.next_token()? {
            Some(Token::Keyword(Keyword::Enum)) => {
                let enum_def = self.parse_enum(description)?;

                self.document.enums.insert(enum_def.name, enum_def);
            }
            Some(Token::Keyword(Keyword::Type)) => {
                let obj_def = self.parse_object_type_definition(description)?;

                self.document.output_objects.insert(obj_def.name, obj_def);
            }
            Some(Token::Keyword(
                kind_keyword @ (Keyword::Query | Keyword::Mutation | Keyword::Subscription),
            )) => {
                let kind = match kind_keyword {
                    Keyword::Query => OperationKind::Query,
                    Keyword::Mutation => OperationKind::Mutation,
                    Keyword::Subscription => OperationKind::Subscription,
                    _ => unreachable!(),
                };

                let operation_def = self.parse_operation(kind)?;

                self.document
                    .operations
                    .insert((operation_def.name, operation_def.kind), operation_def);
            }
            Some(Token::Keyword(Keyword::Fragment)) => {
                let fragment_def = self.parse_fragment_definition()?;

                self.document
                    .fragments
                    .insert(fragment_def.name, fragment_def);
            }
            Some(Token::Keyword(Keyword::Union)) => {
                let union_def = self.parse_union(description)?;

                self.document.unions.insert(union_def.name, union_def);
            }
            Some(Token::Keyword(Keyword::Input)) => {
                let input_def = self.parse_input_object_definition(description)?;

                self.document
                    .input_objects
                    .insert(input_def.name, input_def);
            }
            Some(Token::Keyword(Keyword::Scalar)) => {
                let scalar_def = self.parse_scalar(description)?;

                self.document.scalars.insert(scalar_def.name, scalar_def);
            }
            Some(Token::Keyword(Keyword::Interface)) => {
                let interface_def = self.parse_interface(description)?;

                self.document
                    .interfaces
                    .insert(interface_def.name, interface_def);
            }
            None => return Ok(false),
            Some(token) => todo!("{:?}", token),
        };

        Ok(true)
    }

    fn parse_interface(
        &mut self,
        description: Option<Spur>,
    ) -> Result<Interface, GraphqlParseError> {
        let name = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        let mut fields = Vec::new();

        self.lexer.expect_byte(b'{')?;

        while !self.lexer.consume_byte_if_eq(b'}') {
            fields.push(self.parse_field_definition()?);
        }

        Ok(Interface {
            description,
            name,
            directives,
            fields,
        })
    }

    fn parse_scalar(&mut self, description: Option<Spur>) -> Result<Scalar, GraphqlParseError> {
        let name = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        Ok(Scalar {
            description,
            name,
            directives,
        })
    }

    fn parse_enum(&mut self, description: Option<Spur>) -> Result<Enum, GraphqlParseError> {
        let name = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        self.lexer.expect_byte(b'{')?;

        let mut variants = Vec::new();

        while !self.lexer.consume_byte_if_eq(b'}') {
            variants.push(self.parse_enum_variant()?);
        }

        Ok(Enum {
            description,
            name,
            directives,
            variants,
        })
    }

    fn parse_enum_variant(&mut self) -> Result<EnumVariant, GraphqlParseError> {
        let description = self.parse_optional_description()?;

        let name = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        Ok(EnumVariant {
            description,
            name,
            directives,
        })
    }

    fn parse_input_object_definition(
        &mut self,
        description: Option<Spur>,
    ) -> Result<InputObject, GraphqlParseError> {
        let name = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        let mut fields = Vec::new();

        self.lexer.expect_byte(b'{')?;

        while !self.lexer.consume_byte_if_eq(b'}') {
            fields.push(self.parse_input_field_definition()?);
        }

        Ok(InputObject {
            description,
            name,
            directives,
            fields: Some(fields),
        })
    }

    fn parse_input_field_definition(&mut self) -> Result<InputObjectField, GraphqlParseError> {
        let description = self.parse_optional_description()?;
        let name = self.expect_name()?;

        self.lexer.expect_byte(b':')?;

        let ty = self.parse_type()?;

        let default = if self.lexer.consume_byte_if_eq(b'=') {
            Some(self.parse_value()?)
        } else {
            None
        };

        let directives = self.parse_optional_directives()?;

        Ok(InputObjectField {
            description,
            name,
            ty,
            default,
            directives,
        })
    }

    fn parse_optional_description(&mut self) -> Result<Option<Spur>, GraphqlParseError> {
        match self.lexer.peek_token()? {
            Some(Token::String(string)) => {
                self.lexer.next_token()?;

                Ok(Some(string))
            }
            Some(..) | None => Ok(None),
        }
    }

    fn parse_optional_directives(&mut self) -> Result<Vec<Directive>, GraphqlParseError> {
        let mut directives = Vec::new();

        while self.lexer.consume_byte_if_eq(b'@') {
            let name = self.expect_name()?;
            let arguments = if self.lexer.consume_byte_if_eq(b'(') {
                Some(self.parse_arguments()?)
            } else {
                None
            };

            directives.push(Directive { name, arguments })
        }

        Ok(directives)
    }

    fn parse_arguments(&mut self) -> Result<Vec<Argument>, GraphqlParseError> {
        let mut arguments = Vec::new();

        while !self.lexer.consume_byte_if_eq(b')') {
            let name = self.expect_name()?;

            self.lexer.expect_byte(b':')?;

            let value = self.parse_value()?;

            arguments.push(Argument { name, value })
        }

        Ok(arguments)
    }

    fn parse_object_type_definition(
        &mut self,
        description: Option<Spur>,
    ) -> Result<ObjectType, GraphqlParseError> {
        let name = self.expect_name()?;

        let implements = if self.consume_token_if_eq(Token::Keyword(Keyword::Implements))? {
            self.parse_implements()?
        } else {
            Vec::new()
        };

        let directives = self.parse_optional_directives()?;

        let mut fields = Vec::new();

        self.lexer.expect_byte(b'{')?;

        while !self.lexer.consume_byte_if_eq(b'}') {
            fields.push(self.parse_field_definition()?);
        }

        Ok(ObjectType {
            implements,
            description,
            name,
            directives,
            fields: Some(fields),
        })
    }

    fn parse_implements(&mut self) -> Result<Vec<NamedType>, GraphqlParseError> {
        let mut types = Vec::new();

        types.push(NamedType(self.expect_name()?));

        while self.lexer.consume_byte_if_eq(b'&') {
            types.push(NamedType(self.expect_name()?));
        }

        Ok(types)
    }

    fn parse_operation(&mut self, kind: OperationKind) -> Result<Operation, GraphqlParseError> {
        let name = if let Some(Token::Name(name)) = self.lexer.peek_token()? {
            self.lexer.next_token()?;
            Some(name)
        } else {
            None
        };

        let variable_definitions = if self.lexer.consume_byte_if_eq(b'(') {
            self.parse_variable_definitions()?
        } else {
            Vec::new()
        };

        let directives = self.parse_optional_directives()?;

        self.expect_token(Token::OpenCurlyBrace)?;

        let selection_set = self.parse_selection_set()?;

        Ok(Operation {
            kind,
            name,
            variable_definitions,
            directives,
            selection_set,
        })
    }

    fn parse_variable_definitions(&mut self) -> Result<Vec<VariableDefinition>, GraphqlParseError> {
        let mut variable_definitions = Vec::new();

        while !self.lexer.consume_byte_if_eq(b')') {
            // todo: variables should be their own token. avoid input like `$ a`
            self.expect_token(Token::Dollar)?;

            let name = self.expect_name()?;

            self.expect_token(Token::Colon)?;

            let ty = self.parse_type()?;

            let default = if self.lexer.consume_byte_if_eq(b'=') {
                Some(self.parse_value()?)
            } else {
                None
            };

            variable_definitions.push(VariableDefinition { name, ty, default })
        }

        Ok(variable_definitions)
    }

    fn parse_fragment_definition(&mut self) -> Result<Fragment, GraphqlParseError> {
        let name = self.expect_name()?;

        self.expect_token(Token::Keyword(Keyword::On))?;

        let on = self.expect_name()?;

        let directives = self.parse_optional_directives()?;

        self.expect_token(Token::OpenCurlyBrace)?;

        let selection_set = self.parse_selection_set()?;

        Ok(Fragment {
            name,
            on,
            directives,
            selection_set,
        })
    }

    fn parse_selection_set(&mut self) -> Result<Vec<Selection>, GraphqlParseError> {
        let mut selection_set = Vec::new();

        while !self.lexer.consume_byte_if_eq(b'}') {
            if self.consume_token_if_eq(Token::DotDotDot)? {
                selection_set.push(self.parse_inline_or_spread_fragment()?);
                continue;
            }

            selection_set.push(self.parse_fragment_field()?);
        }

        Ok(selection_set)
    }

    fn parse_inline_or_spread_fragment(&mut self) -> Result<Selection, GraphqlParseError> {
        if self.consume_token_if_eq(Token::Keyword(Keyword::On))? {
            return self.parse_inline_fragment();
        }

        let name = self.expect_name()?;
        let directives = self.parse_optional_directives()?;

        Ok(Selection::FragmentSpread { name, directives })
    }

    fn parse_inline_fragment(&mut self) -> Result<Selection, GraphqlParseError> {
        let on = self.expect_name()?;
        let directives = self.parse_optional_directives()?;

        self.expect_token(Token::OpenCurlyBrace)?;

        let selection_set = self.parse_selection_set()?;

        Ok(Selection::InlineFragment {
            on,
            directives,
            selection_set,
        })
    }

    fn parse_fragment_field(&mut self) -> Result<Selection, GraphqlParseError> {
        let alias_or_name = self.expect_name()?;

        let (alias, name) = if self.lexer.consume_byte_if_eq(b':') {
            (Some(alias_or_name), self.expect_name()?)
        } else {
            (None, alias_or_name)
        };

        let arguments = if self.lexer.consume_byte_if_eq(b'(') {
            Some(self.parse_arguments()?)
        } else {
            None
        };

        let directives = self.parse_optional_directives()?;

        let selection_set = if self.lexer.consume_byte_if_eq(b'{') {
            Some(self.parse_selection_set()?)
        } else {
            None
        };

        Ok(Selection::Field {
            alias,
            name,
            arguments,
            directives,
            selection_set,
        })
    }

    fn parse_union(&mut self, description: Option<Spur>) -> Result<Union, GraphqlParseError> {
        let name = self.expect_name()?;
        let directives = self.parse_optional_directives()?;

        self.lexer.expect_byte(b'=')?;

        let mut types = Vec::new();

        types.push(NamedType(self.expect_name()?));

        while self.lexer.consume_byte_if_eq(b'|') {
            types.push(NamedType(self.expect_name()?));
        }

        Ok(Union {
            name,
            description,
            directives,
            types,
        })
    }

    fn parse_field_definition(&mut self) -> Result<FieldDefinition, GraphqlParseError> {
        let description = self.parse_optional_description()?;
        let name = self.expect_name()?;

        let arguments = self.parse_optional_field_arguments()?;

        self.lexer.expect_byte(b':')?;

        let ty = self.parse_type()?;

        let directives = self.parse_optional_directives()?;

        Ok(FieldDefinition {
            description,
            name,
            arguments,
            ty,
            directives,
        })
    }

    fn parse_optional_field_arguments(
        &mut self,
    ) -> Result<Option<Vec<InputObjectField>>, GraphqlParseError> {
        Ok(if self.lexer.consume_byte_if_eq(b'(') {
            Some(self.parse_field_arguments()?)
        } else {
            None
        })
    }

    fn parse_field_arguments(&mut self) -> Result<Vec<InputObjectField>, GraphqlParseError> {
        let mut arguments = Vec::new();

        while !self.lexer.consume_byte_if_eq(b')') {
            arguments.push(self.parse_input_field_definition()?);
        }

        Ok(arguments)
    }

    fn parse_type(&mut self) -> Result<Type, GraphqlParseError> {
        let mut base = match self.lexer.next_token()? {
            Some(Token::Name(name)) => Type::Named {
                name,
                nullable: true,
            },
            Some(Token::OpenSquareBrace) => {
                let ty = Type::List {
                    base: Box::new(self.parse_type()?),
                    nullable: true,
                };
                self.lexer.expect_byte(b']')?;

                ty
            }
            _ => todo!(),
        };

        if self.lexer.consume_byte_if_eq(b'!') {
            base.set_nonnullable();
        }

        Ok(base)
    }
}
