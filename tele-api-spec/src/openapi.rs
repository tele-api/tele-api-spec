use indexmap::{IndexMap, indexmap};
use openapiv3::{
    AnySchema,
    ArrayType,
    BooleanType,
    ExternalDocumentation,
    IntegerType,
    MediaType,
    NumberType,
    ObjectType,
    OpenAPI,
    Operation,
    PathItem,
    Paths,
    ReferenceOr,
    RequestBody,
    Response,
    Responses,
    Schema,
    SchemaData,
    SchemaKind,
    StatusCode,
    StringType,
    Type,
};
use tele_api_parser::{Argument, Field, MethodArgs, ObjectData, Parsed, Type as ParserType};

const BASE_SCHEMA: &str = include_str!("../base-schema.yml");
const FORM_URL_ENCODED: &str = "application/x-www-form-urlencoded";
const JSON: &str = "application/json";
const FORM_DATA: &str = "multipart/form-data";

pub fn generate(parsed: Parsed) -> OpenAPI {
    let mut api: OpenAPI = serde_yml::from_str(BASE_SCHEMA).expect("Base schema is invalid");

    let success = api
        .components
        .as_mut()
        .unwrap()
        .schemas
        .shift_remove("Success")
        .unwrap();

    let mut schemas = indexmap![];
    for object in parsed.objects {
        let schema_kind = match object.data {
            ObjectData::Fields(fields) => {
                let (properties, required) = make_properties_and_required(fields);
                SchemaKind::Type(Type::Object(ObjectType {
                    properties,
                    required,
                    ..ObjectType::default()
                }))
            }
            ObjectData::Elements(elements) => {
                let any_of = elements
                    .into_iter()
                    .map(ParserType::into_schema)
                    .map(ReferenceOr::unbox)
                    .collect();
                SchemaKind::AnyOf { any_of }
            }
            ObjectData::Unknown => SchemaKind::Any(AnySchema::default()),
        };

        schemas.insert(
            object.name,
            ReferenceOr::Item(Schema {
                schema_data: SchemaData {
                    description: Some(object.description),
                    external_docs: Some(ExternalDocumentation {
                        url: object.docs_link,
                        ..ExternalDocumentation::default()
                    }),
                    ..SchemaData::default()
                },
                schema_kind,
            }),
        );
    }

    let mut paths = indexmap![];
    for method in parsed.methods {
        let (file_uploading, has_args) = match method.args {
            MethodArgs::No => (false, false),
            MethodArgs::Yes(_) => (false, true),
            MethodArgs::WithMultipart(_) => (true, true),
        };

        let mut content = indexmap![];
        let (properties, required) = match method.args {
            MethodArgs::Yes(args) | MethodArgs::WithMultipart(args) => {
                make_properties_and_required(args)
            }
            MethodArgs::No => (indexmap![], vec![]),
        };

        for content_type in [
            Some(FORM_URL_ENCODED).filter(|_| !file_uploading),
            Some(FORM_DATA),
            Some(JSON).filter(|_| !file_uploading),
        ]
        .iter()
        .flatten()
        {
            content.insert(
                (*content_type).to_string(),
                MediaType {
                    schema: Some(ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Type(Type::Object(ObjectType {
                            properties: properties.clone(),
                            required: required.clone(),
                            ..ObjectType::default()
                        })),
                    })),
                    ..MediaType::default()
                },
            );
        }

        let mut success = success.clone();
        if let ReferenceOr::Item(item) = &mut success {
            if let SchemaKind::Type(Type::Object(object)) = &mut item.schema_kind {
                object
                    .properties
                    .insert("result".to_string(), method.return_type.into_schema());
            }
        }

        let operation = Operation {
            description: Some(method.description),
            request_body: if has_args {
                Some(ReferenceOr::Item(RequestBody {
                    content,
                    required: true,
                    ..RequestBody::default()
                }))
            } else {
                None
            },
            responses: Responses {
                default:    Some(ReferenceOr::Item(Response {
                    content: indexmap! {
                        JSON.to_string() => MediaType {
                            schema: Some(ReferenceOr::Reference { reference: "#/components/schemas/Error".to_string() }),
                            ..MediaType::default()
                        }
                    },
                    extensions: IndexMap::new(),
                    ..Response::default()
                })),
                responses:  indexmap! {
                    StatusCode::Code(200) => ReferenceOr::Item(Response {
                        content: indexmap! {
                            JSON.to_string() => MediaType {
                                schema: Some(success),
                                ..MediaType::default()
                            }
                        },
                        extensions: IndexMap::new(),
                        ..Response::default()
                    }),
                },
                extensions: IndexMap::new(),
            },
            external_docs: Some(ExternalDocumentation {
                url: method.docs_link,
                ..ExternalDocumentation::default()
            }),
            ..Operation::default()
        };

        let item = PathItem {
            post: Some(operation),
            ..PathItem::default()
        };

        paths.insert(format!("/{}", method.name), ReferenceOr::Item(item));
    }

    api.info.version = parsed.version.to_string();
    api.paths = Paths {
        paths,
        extensions: IndexMap::new(),
    };
    if let Some(components) = &mut api.components {
        components.schemas.extend(schemas);
    }

    api
}

fn make_properties_and_required<T>(
    common: Vec<T>,
) -> (IndexMap<String, ReferenceOr<Box<Schema>>>, Vec<String>)
where T: Into<CommonContent> {
    common.into_iter().map(Into::into).fold(
        (indexmap![], vec![]),
        |(mut properties, mut required), content| {
            if content.required {
                required.push(content.name.clone());
            }

            let ref_or_schema_parts = content.kind.into_ref_or_schema_parts();
            let ref_or_schema = match ref_or_schema_parts {
                ReferenceOr::Item(SchemaParts {
                    default,
                    kind: schema_kind,
                }) => ReferenceOr::Item(Box::new(Schema {
                    schema_data: SchemaData {
                        description: Some(content.description),
                        default,
                        ..SchemaData::default()
                    },
                    schema_kind,
                })),
                ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
            };
            properties.insert(content.name, ref_or_schema);

            (properties, required)
        },
    )
}

pub fn generate_simplified(parsed: Parsed) -> OpenAPI {
    let mut api: OpenAPI = serde_yml::from_str(BASE_SCHEMA).expect("Base schema is invalid");

    let success = api
        .components
        .as_mut()
        .unwrap()
        .schemas
        .shift_remove("Success")
        .unwrap();

    let mut schemas = indexmap![];
    for object in parsed.objects {
        let schema_kind = match object.data {
            ObjectData::Fields(fields) => {
                let (properties, required) = make_properties_and_required_simplified(fields);
                SchemaKind::Type(Type::Object(ObjectType {
                    properties,
                    required,
                    ..ObjectType::default()
                }))
            }
            ObjectData::Elements(elements) => {
                if let Some(first_element) = elements.into_iter().next() {
                    match first_element.into_schema_first() {
                        ReferenceOr::Item(boxed_schema) => boxed_schema.schema_kind,
                        ReferenceOr::Reference { reference: _ } => SchemaKind::Any(AnySchema::default()),
                    }
                } else {
                    SchemaKind::Any(AnySchema::default())
                }
            }
            ObjectData::Unknown => SchemaKind::Any(AnySchema::default()),
        };

        schemas.insert(
            object.name,
            ReferenceOr::Item(Schema {
                schema_data: SchemaData {
                    description: Some(object.description),
                    external_docs: Some(ExternalDocumentation {
                        url: object.docs_link,
                        ..ExternalDocumentation::default()
                    }),
                    ..SchemaData::default()
                },
                schema_kind,
            }),
        );
    }

    let mut paths = indexmap![];
    for method in parsed.methods {
        let (file_uploading, has_args) = match method.args {
            MethodArgs::No => (false, false),
            MethodArgs::Yes(_) => (false, true),
            MethodArgs::WithMultipart(_) => (true, true),
        };

        let mut content = indexmap![];
        let (properties, required) = match method.args {
            MethodArgs::Yes(args) | MethodArgs::WithMultipart(args) => {
                make_properties_and_required_simplified(args)
            }
            MethodArgs::No => (indexmap![], vec![]),
        };

        for content_type in [
            Some(FORM_URL_ENCODED).filter(|_| !file_uploading),
            Some(FORM_DATA),
            Some(JSON).filter(|_| !file_uploading),
        ]
        .iter()
        .flatten()
        {
            content.insert(
                (*content_type).to_string(),
                MediaType {
                    schema: Some(ReferenceOr::Item(Schema {
                        schema_data: SchemaData::default(),
                        schema_kind: SchemaKind::Type(Type::Object(ObjectType {
                            properties: properties.clone(),
                            required: required.clone(),
                            ..ObjectType::default()
                        })),
                    })),
                    ..MediaType::default()
                },
            );
        }

        let mut success = success.clone();
        if let ReferenceOr::Item(item) = &mut success {
            if let SchemaKind::Type(Type::Object(object)) = &mut item.schema_kind {
                object
                    .properties
                    .insert("result".to_string(), method.return_type.into_schema_first());
            }
        }

        let operation = Operation {
            description: Some(method.description),
            request_body: if has_args {
                Some(ReferenceOr::Item(RequestBody {
                    content,
                    required: true,
                    ..RequestBody::default()
                }))
            } else {
                None
            },
            responses: Responses {
                default:    Some(ReferenceOr::Item(Response {
                    content: indexmap! {
                        JSON.to_string() => MediaType {
                            schema: Some(ReferenceOr::Reference { reference: "#/components/schemas/Error".to_string() }),
                            ..MediaType::default()
                        }
                    },
                    extensions: IndexMap::new(),
                    ..Response::default()
                })),
                responses:  indexmap! {
                    StatusCode::Code(200) => ReferenceOr::Item(Response {
                        content: indexmap! {
                            JSON.to_string() => MediaType {
                                schema: Some(success),
                                ..MediaType::default()
                            }
                        },
                        extensions: IndexMap::new(),
                        ..Response::default()
                    }),
                },
                extensions: IndexMap::new(),
            },
            external_docs: Some(ExternalDocumentation {
                url: method.docs_link,
                ..ExternalDocumentation::default()
            }),
            ..Operation::default()
        };

        let item = PathItem {
            post: Some(operation),
            ..PathItem::default()
        };

        paths.insert(format!("/{}", method.name), ReferenceOr::Item(item));
    }

    api.info.version = parsed.version.to_string();
    api.paths = Paths {
        paths,
        extensions: IndexMap::new(),
    };
    if let Some(components) = &mut api.components {
        components.schemas.extend(schemas);
    }

    api
}

// Helper function for simplified schema generation
fn make_properties_and_required_simplified<T>(
    common: Vec<T>,
) -> (IndexMap<String, ReferenceOr<Box<Schema>>>, Vec<String>)
where T: Into<CommonContent> {
    common.into_iter().map(Into::into).fold(
        (indexmap![], vec![]),
        |(mut properties, mut required), content| {
            if content.required {
                required.push(content.name.clone());
            }

            let ref_or_schema_parts = content.kind.into_ref_or_schema_parts_first();
            let ref_or_schema = match ref_or_schema_parts {
                ReferenceOr::Item(SchemaParts {
                    default,
                    kind: schema_kind,
                }) => ReferenceOr::Item(Box::new(Schema {
                    schema_data: SchemaData {
                        description: Some(content.description),
                        default,
                        ..SchemaData::default()
                    },
                    schema_kind,
                })),
                ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
            };
            properties.insert(content.name, ref_or_schema);

            (properties, required)
        },
    )
}

trait TypeExt: Sized {
    fn into_ref_or_schema_parts(self) -> ReferenceOr<SchemaParts>;

    fn into_schema(self) -> ReferenceOr<Box<Schema>>;

    fn into_ref_or_schema_parts_first(self) -> ReferenceOr<SchemaParts>;

    fn into_schema_first(self) -> ReferenceOr<Box<Schema>>;
}

impl TypeExt for ParserType {
    fn into_ref_or_schema_parts(self) -> ReferenceOr<SchemaParts> {
        let default = match &self {
            Self::Bool { default } => default.map(Into::into),
            Self::Integer { default, .. } => default.map(Into::into),
            Self::String { default, .. } => default.clone().map(Into::into),
            _ => None,
        };

        let schema_kind = match self {
            this @ (Self::Integer { .. }
            | Self::String { .. }
            | Self::Bool { .. }
            | Self::Float
            | Self::Array(_)) => {
                let schema_type = match this {
                    Self::Integer {
                        min, max, one_of, ..
                    } => Type::Integer(IntegerType {
                        minimum: min,
                        maximum: max,
                        enumeration: one_of.into_iter().map(Some).collect(),
                        ..IntegerType::default()
                    }),
                    Self::String {
                        one_of,
                        min_len,
                        max_len,
                        ..
                    } => Type::String(StringType {
                        min_length: min_len.map(|x| x as usize),
                        max_length: max_len.map(|x| x as usize),
                        enumeration: one_of.into_iter().map(Some).collect(),
                        ..StringType::default()
                    }),
                    Self::Bool { .. } => Type::Boolean(BooleanType::default()),
                    Self::Float => Type::Number(NumberType::default()),
                    Self::Array(array) => Type::Array(ArrayType {
                        items:        Some(array.into_schema()),
                        min_items:    None,
                        max_items:    None,
                        unique_items: false,
                    }),
                    _ => unreachable!(),
                };
                SchemaKind::Type(schema_type)
            }
            Self::Or(types) => SchemaKind::AnyOf {
                any_of: types
                    .into_iter()
                    .map(Self::into_schema)
                    .map(ReferenceOr::unbox)
                    .collect(),
            },
            Self::Object(reference) => {
                return ReferenceOr::Reference {
                    reference: format!("#/components/schemas/{reference}"),
                };
            }
        };

        ReferenceOr::Item(SchemaParts {
            default,
            kind: schema_kind,
        })
    }

    fn into_schema(self) -> ReferenceOr<Box<Schema>> {
        match self.into_ref_or_schema_parts() {
            ReferenceOr::Item(parts) => ReferenceOr::Item(Box::new(Schema {
                schema_data: SchemaData {
                    default: parts.default,
                    ..SchemaData::default()
                },
                schema_kind: parts.kind,
            })),
            ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
        }
    }

    fn into_ref_or_schema_parts_first(self) -> ReferenceOr<SchemaParts> {
        let default = match &self {
            Self::Bool { default } => default.map(Into::into),
            Self::Integer { default, .. } => default.map(Into::into),
            Self::String { default, .. } => default.clone().map(Into::into),
            _ => None,
        };

        let schema_kind = match self {
            this @ (Self::Integer { .. }
            | Self::String { .. }
            | Self::Bool { .. }
            | Self::Float
            | Self::Array(_)) => {
                let schema_type = match this {
                    Self::Integer {
                        min, max, one_of, ..
                    } => Type::Integer(IntegerType {
                        minimum: min,
                        maximum: max,
                        enumeration: one_of.into_iter().map(Some).collect(),
                        ..IntegerType::default()
                    }),
                    Self::String {
                        one_of,
                        min_len,
                        max_len,
                        ..
                    } => Type::String(StringType {
                        min_length: min_len.map(|x| x as usize),
                        max_length: max_len.map(|x| x as usize),
                        enumeration: one_of.into_iter().map(Some).collect(),
                        ..StringType::default()
                    }),
                    Self::Bool { .. } => Type::Boolean(BooleanType::default()),
                    Self::Float => Type::Number(NumberType::default()),
                    Self::Array(array) => Type::Array(ArrayType {
                        items:        Some(array.into_schema_first()),
                        min_items:    None,
                        max_items:    None,
                        unique_items: false,
                    }),
                    _ => unreachable!(),
                };
                SchemaKind::Type(schema_type)
            }
            Self::Or(types) => {
                if let Some(first_type) = types.into_iter().next() {
                    return first_type.into_ref_or_schema_parts_first();
                } else {
                    SchemaKind::Any(AnySchema::default())
                }
            },
            Self::Object(reference) => {
                return ReferenceOr::Reference {
                    reference: format!("#/components/schemas/{reference}"),
                };
            }
        };

        ReferenceOr::Item(SchemaParts {
            default,
            kind: schema_kind,
        })
    }

    fn into_schema_first(self) -> ReferenceOr<Box<Schema>> {
        match self.into_ref_or_schema_parts_first() {
            ReferenceOr::Item(parts) => ReferenceOr::Item(Box::new(Schema {
                schema_data: SchemaData {
                    default: parts.default,
                    ..SchemaData::default()
                },
                schema_kind: parts.kind,
            })),
            ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
        }
    }
}

struct SchemaParts {
    default: Option<serde_json::Value>,
    kind:    SchemaKind,
}

struct CommonContent {
    name:        String,
    description: String,
    required:    bool,
    kind:        ParserType,
}

impl From<Argument> for CommonContent {
    fn from(arg: Argument) -> Self {
        Self {
            name:        arg.name,
            description: arg.description,
            required:    arg.required,
            kind:        arg.kind,
        }
    }
}

impl From<Field> for CommonContent {
    fn from(field: Field) -> Self {
        Self {
            name:        field.name,
            description: field.description,
            required:    field.required,
            kind:        field.kind,
        }
    }
}
