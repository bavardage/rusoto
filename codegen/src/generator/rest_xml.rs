use inflector::Inflector;

use botocore::{Member, Operation, Service, Shape, ShapeType};
use super::GenerateProtocol;
use super::generate_field_name;

pub struct RestXmlGenerator;

impl GenerateProtocol for RestXmlGenerator {
    fn generate_methods(&self, service: &Service) -> String {
        service.operations.values().map(|operation| {
            format!(
                "{documentation}
                #[allow(unused_variables, warnings)]
                {method_signature} {{                  
					
                    let mut params = Params::new();
                    params.put(\"Action\", \"{operation_name}\");
                    let mut payload: Option<Vec<u8>> = None;

                    {serialize_input}

                    let mut request_uri = \"{request_uri}\".to_string();

                    {modify_uri}

					let mut request = SignedRequest::new(\"{http_method}\", \"{endpoint_prefix}\", self.region, &request_uri);

					{set_headers}

					if payload.is_some() {{
						request.set_payload(Some(payload.as_ref().unwrap().as_slice()));
					}}

                    request.set_params(params);
                    request.sign(&try!(self.credentials_provider.credentials()));

                    let response = try!(self.dispatcher.dispatch(&request));

                    match response.status {{
                        200 => {{
                            {parse_response}
                        }},
                        _ => Err({error_type}::from_body(&response.body))
                    }}
                }}
                ",
                documentation = generate_documentation(operation),
                http_method = &operation.http.method,
                endpoint_prefix = &service.metadata.endpoint_prefix,
                method_signature = generate_method_signature(operation),
                operation_name = &operation.name,
                error_type = operation.error_type_name(),
                request_uri = &operation.http.request_uri.replace("+",""),
                serialize_input = generate_method_input_serialization(service, operation).unwrap_or("".to_string()),
                modify_uri = generate_uri_modification(service, operation).unwrap_or("".to_string()),
                set_headers = generate_headers(service, operation).unwrap_or("".to_string()),
                parse_response = generate_response_parser(service, operation)
            )
        }).collect::<Vec<String>>().join("\n")
    }

    fn generate_prelude(&self, _service: &Service) -> String {
        "use std::str::{FromStr};
        use std::collections::HashMap;

        use xml::EventReader;

        use param::{Params, ServiceParams};

        use signature::SignedRequest;
        use xml::reader::events::XmlEvent;
        use xmlutil::{Next, Peek, XmlParseError, XmlResponse};
        use xmlutil::{peek_at_name, characters, end_element, start_element, skip_tree};
        use xmlerror::*;

        enum DeserializerNext {
            Close,
            Skip,
            Element(String),
        }
        ".to_owned()
    }

    fn generate_struct_attributes(&self) -> String {
        "#[derive(Debug, Default)]".to_owned()
    }

    fn generate_support_types(&self, name: &str, shape: &Shape, _service: &Service) -> Option<String> {

    	// (most) requests never need XML serialization or deserialization, so don't generate the type
    	if name != "RestoreRequest" && name.ends_with("Request") {
    		return None;
    	}

    	let mut parts: Vec<String> = Vec::with_capacity(2);

       	parts.push(format!("
            struct {name}Deserializer;
            impl {name}Deserializer {{
                #[allow(unused_variables)]
                fn deserialize<'a, T: Peek + Next>(tag_name: &str, stack: &mut T)
                -> Result<{name}, XmlParseError> {{
                    {deserializer_body}
                }}
            }}",
            name = name,
            deserializer_body = generate_deserializer_body(name, shape)
        ));

       	// Output types never need to be serialized
       	if !name.ends_with("Output") {
       		parts.push(format!("
            	struct {name}Serializer;
            	impl {name}Serializer {{
            	    {serializer_signature} {{
            	        {serializer_body}
            	    }}
            	}}
            	",
            	name = name,
            	serializer_body = generate_serializer_body(shape),
            	serializer_signature = generate_serializer_signature(name, shape),
        	))
       }

       Some(parts.join("\n"))

    }

    fn timestamp_type(&self) -> &'static str {
        "String"
    }
}

fn generate_documentation(operation: &Operation) -> String {
    match operation.documentation {
        Some(ref docs) => format!("#[doc=\"{}\"]", docs.replace("\"", "\\\"").replace("C:\\", "C:\\\\")),
        None => "".to_owned(),
    }
}

fn generate_method_input_serialization(service: &Service, operation: &Operation) -> Option<String> {

	// nothing to do if there's no input type
	if operation.input.is_none() {
		return None;
	}

	let input_shape = service.shapes.get(&operation.input.as_ref().unwrap().shape).unwrap();

	let mut parts: Vec<String> = Vec::new();

	// the payload field determines which member of the input shape is sent as the request body (if any)
	if input_shape.payload.is_some() {
		parts.push(generate_payload_serialization(input_shape));
	}

	Some(parts.join("\n"))
}


fn generate_uri_modification(service: &Service, operation: &Operation) -> Option<String> {

	// nothing to do if there's no input type
	if operation.input.is_none() {
		return None;
	}

	let shape = service.shapes.get(&operation.input.as_ref().unwrap().shape).unwrap();

	Some(shape.members.as_ref().unwrap().iter().filter_map(|(member_name, member)| {
		if member.location.is_none() {
			return None;
		}
		match &member.location.as_ref().unwrap()[..] {
			"uri" => {
				if shape.required(&member_name) {
					Some(format!("request_uri = request_uri.replace(\"{{{location_name}}}\", &input.{field_name});",
						location_name = member.location_name.as_ref().unwrap(),
						field_name = member_name.to_snake_case()))
				} else {
					Some(format!("request_uri = request_uri.replace(\"{{{location_name}}}\", &input.{field_name}.unwrap());",
						location_name = member.location_name.as_ref().unwrap(),
						field_name = member_name.to_snake_case()))
				}
			},
			_ => None
		}
	}).collect::<Vec<String>>().join("\n"))
}

fn generate_headers(service: &Service, operation: &Operation) -> Option<String> {

	// nothing to do if there's no input type
	if operation.input.is_none() {
		return None;
	}

	let shape = service.shapes.get(&operation.input.as_ref().unwrap().shape).unwrap();

	Some(shape.members.as_ref().unwrap().iter().filter_map(|(member_name, member)| {
		if member.location.is_none() {
			return None;
		}
		match &member.location.as_ref().unwrap()[..] {
			"header" => {
				if shape.required(&member_name) {
					Some(format!("request.add_header(\"{location_name}\", &input.{field_name});",
						location_name = member.location_name.as_ref().unwrap(),
						field_name = member_name.to_snake_case()))        			
				} else {
					Some(format!("
						if let Some(ref {field_name}) = input.{field_name} {{
            				request.add_header(\"{location_name}\", &{field_name}.to_string());
						}}",
						location_name = member.location_name.as_ref().unwrap(),
						field_name = member_name.to_snake_case()))
				}
			},
			_ => None
		}
	}).collect::<Vec<String>>().join("\n"))
}

fn generate_payload_serialization(shape: &Shape) -> String {
	let payload_field = shape.payload.as_ref().unwrap();
	let payload_member = shape.members.as_ref().unwrap().get(payload_field).unwrap();

	// if the member is 'streaming', it's a Vec<u8> that should just be delivered as the body
	if payload_member.streaming() {
		format!("payload = Some(input.{}.clone().unwrap());", payload_field.to_snake_case())
	} 
	// otherwise serialize the object to XML and use that as the payload
	else {
		// some payload types are not required members of their shape
		if shape.required(&payload_field) {
			format!(
				"payload = Some({xml_type}Serializer::serialize(\"{xml_type}\", &input.{payload_field}).into_bytes());",
       	    	payload_field = payload_field.to_snake_case(),
           		xml_type = payload_member.shape)			
		} else {
			format!(
   	        	"if input.{payload_field}.is_some() {{
					payload = Some({xml_type}Serializer::serialize(\"{xml_type}\", input.{payload_field}.as_ref().unwrap()).into_bytes());
   	        	}}",
       	    	payload_field = payload_field.to_snake_case(),
           		xml_type = payload_member.shape)
		}
	}
}

fn generate_response_parser(service: &Service, operation: &Operation) -> String {

    if operation.output.is_none() {
        return "Ok(())".to_string();
    }

    let output_shape = &operation.output.as_ref().unwrap().shape;

    format!("
        let mut result;


        if response.body.is_empty() {{
            result = {output_shape}::default();
        }} else {{
            let mut reader = EventReader::from_str(&response.body);
            let mut stack = XmlResponse::new(reader.events().peekable());
            let _start_document = stack.next();         
            let actual_tag_name = try!(peek_at_name(&mut stack));
            result = try!({output_shape}Deserializer::deserialize(&actual_tag_name, &mut stack));
        }}

        {parse_response_headers}

        Ok(result)

        ",
        output_shape = output_shape,
        parse_response_headers = generate_response_headers_parser(service, operation).unwrap_or("".to_string()))
}

fn generate_response_headers_parser(service: &Service, operation: &Operation) -> Option<String> {

    // nothing to do if there's no output type
    if operation.output.is_none() {
        return None;
    }

    let shape = service.shapes.get(&operation.output.as_ref().unwrap().shape).unwrap();

    Some(shape.members.as_ref().unwrap().iter().filter_map(|(member_name, member)| {
        if member.location.is_none() || member.location.as_ref().unwrap() != "header" {
            return None;
        }

        let member_shape_name = &member.shape;
        let member_shape = service.shapes.get(member_shape_name).unwrap();

        if shape.required(&member_name) {
            Some(format!("
                let value = response.headers.get(\"{location_name}\").as_ref().unwrap().to_owned();
                result.{field_name} = {primitive_parser}",
                location_name = member.location_name.as_ref().unwrap(),
                field_name = member_name.to_snake_case(),
                primitive_parser = generate_header_primitive_parser(&member_shape)))
        } else {
            Some(format!("
                if let Some({field_name}) = response.headers.get(\"{location_name}\") {{
                    let value = {field_name}.to_owned();
                    result.{field_name} = Some({primitive_parser})
                }}",
                location_name = member.location_name.as_ref().unwrap(),
                field_name = member_name.to_snake_case(),
                primitive_parser = generate_header_primitive_parser(&member_shape)))
        }


    }).collect::<Vec<String>>().join("\n"))
}

/// Parse a primitive type from the response headers
fn generate_header_primitive_parser(shape: &Shape) -> String {
    let statement =  match shape.shape_type {
        ShapeType::String | ShapeType::Timestamp => "value",
        ShapeType::Integer => "i32::from_str(&value).unwrap()",
        ShapeType::Long => "i64::from_str(&value).unwrap()",
        ShapeType::Double => "f64::from_str(&value).unwrap()",
        ShapeType::Float => "f32::from_str(&value).unwrap()",
        ShapeType::Boolean => "bool::from_str(&value).unwrap()",
        _ => panic!("Unknown primitive shape type"),
    };

    statement.to_string()

}

fn generate_method_signature(operation: &Operation) -> String {
    if operation.input.is_some() {
        format!(
            "pub fn {operation_name}(&self, input: &{input_type}) -> Result<{output_type}, {error_type}>",
            input_type = operation.input.as_ref().unwrap().shape,
            operation_name = operation.name.to_snake_case(),
            output_type = &operation.output_shape_or("()"),
            error_type = operation.error_type_name(),
        )
    } else {
        format!(
            "pub fn {operation_name}(&self) -> Result<{output_type}, {error_type}>",
            operation_name = operation.name.to_snake_case(),
            error_type = operation.error_type_name(),
            output_type = &operation.output_shape_or("()"),
        )
    }
}

fn generate_deserializer_body(name: &str, shape: &Shape) -> String {
    match shape.shape_type {
        ShapeType::List => generate_list_deserializer(shape),
        ShapeType::Map => generate_map_deserializer(shape),        
        ShapeType::Structure => generate_struct_deserializer(name, shape),
        _ => generate_primitive_deserializer(shape),
    }
}

fn generate_list_deserializer(shape: &Shape) -> String {

    let location_name = shape.member.as_ref().and_then(|m| m.location_name.as_ref()).map(|name| &name[..]).unwrap_or(shape.member());

    format!(
        "
        let mut obj = vec![];
        try!(start_element(tag_name, stack));

        loop {{
            let next_event = match stack.peek() {{
                Some(&XmlEvent::EndElement {{ .. }}) => DeserializerNext::Close,
                Some(&XmlEvent::StartElement {{ ref name, .. }}) => DeserializerNext::Element(name.local_name.to_owned()),
                _ => DeserializerNext::Skip,
            }};

            match next_event {{
                DeserializerNext::Element(name) => {{
                    if name == \"{location_name}\" {{
                        obj.push(try!({member_name}Deserializer::deserialize(\"{location_name}\", stack)));
                    }} else {{
                        skip_tree(stack);
                    }}
                }},
                DeserializerNext::Close => {{
                    try!(end_element(tag_name, stack));
                    break;
                }}
                DeserializerNext::Skip => {{ stack.next(); }},
            }}
        }}

        Ok(obj)
        ",
        location_name = location_name,
        member_name = generate_member_name(&shape)
    )
}

fn generate_member_name(shape: &Shape) -> String {
	match &shape.member()[..] {
		"Error" => "S3Error".to_owned(),
		_ => shape.member().to_owned()
	}
}

fn generate_map_deserializer(shape: &Shape) -> String {
    let key = shape.key.as_ref().unwrap();
    let value = shape.value.as_ref().unwrap();

    format!(
        "
        let mut obj = HashMap::new();

        while try!(peek_at_name(stack)) == tag_name {{
            try!(start_element(tag_name, stack));
            let key = try!({key_type_name}Deserializer::deserialize(\"{key_tag_name}\", stack));
            let value = try!({value_type_name}Deserializer::deserialize(\"{value_tag_name}\", stack));
            obj.insert(key, value);
            try!(end_element(tag_name, stack));
        }}

        Ok(obj)
        ",
        key_tag_name = key.tag_name(),
        key_type_name = key.shape,
        value_tag_name = value.tag_name(),
        value_type_name = value.shape,
    )
}

fn generate_primitive_deserializer(shape: &Shape) -> String {
    let statement =  match shape.shape_type {
        ShapeType::String | ShapeType::Timestamp => "try!(characters(stack))",
        ShapeType::Integer => "i32::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Long => "i64::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Double => "f64::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Float => "f32::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Blob => "try!(characters(stack)).into_bytes()",
        ShapeType::Boolean => "bool::from_str(try!(characters(stack)).as_ref()).unwrap()",
        _ => panic!("Unknown primitive shape type"),
    };

    format!(
        "try!(start_element(tag_name, stack));
        let obj = {statement};
        try!(end_element(tag_name, stack));

        Ok(obj)
        ",
        statement = statement,
    )
}

fn generate_struct_deserializer(name: &str, shape: &Shape) -> String {

    let mut needs_xml_deserializer = false;

    // don't generate an xml deserializer if we don't need to
    for (_, member) in shape.members.as_ref().unwrap().iter() {
        match member.location.as_ref().map(String::as_ref) {
            Some("header") | Some("headers") => {},
            _ => needs_xml_deserializer = true
        }
    }

    if !needs_xml_deserializer || shape.members.as_ref().unwrap().is_empty() {
        return format!(
            "try!(start_element(tag_name, stack));
            stack.next();

            let obj = {name}::default();

            try!(end_element(tag_name, stack));
            stack.next();

            Ok(obj)
            ",
            name = name,
        );
    }

    format!(
        "try!(start_element(tag_name, stack));

        let mut obj = {name}::default();

        loop {{
            let next_event = match stack.peek() {{
                Some(&XmlEvent::EndElement {{ .. }}) => DeserializerNext::Close,   // TODO verify that we received the expected tag?
                Some(&XmlEvent::StartElement {{ ref name, .. }}) => DeserializerNext::Element(name.local_name.to_owned()),
                _ => DeserializerNext::Skip,
            }};

            match next_event {{
                DeserializerNext::Element(name) => {{
                    match &name[..] {{
                        {struct_field_deserializers}
                        _ => skip_tree(stack),
                    }}
                }},
                DeserializerNext::Close => break,
                DeserializerNext::Skip => {{ stack.next(); }},
            }}
        }}

        try!(end_element(tag_name, stack));

        Ok(obj)
        ",
        name = name,
        struct_field_deserializers = generate_struct_field_deserializers(shape),
    )
}

fn generate_struct_field_deserializers(shape: &Shape) -> String {
    shape.members.as_ref().unwrap().iter().filter_map(|(member_name, member)| {
        // look up member.shape in all_shapes.  use that shape.member.location_name
        let location_name = member.location_name.as_ref().unwrap_or(member_name);

        if member.deprecated() {
        	return None
        }

        let parse_expression = generate_struct_field_parse_expression(shape, member_name, member, member.location_name.as_ref());
        Some(format!(
            "\"{location_name}\" => {{
                obj.{field_name} = {parse_expression};
            }}",
            field_name = generate_field_name(member_name),
            parse_expression = parse_expression,
            location_name = location_name,
        ))

    }).collect::<Vec<String>>().join("\n")
}

fn generate_struct_field_parse_expression(
    shape: &Shape,
    member_name: &str,
    member: &Member,
    location_name: Option<&String>,
) -> String {

    let location_to_use = match location_name {
        Some(loc) => loc.to_string(),
        None => member_name.to_string(),
    };
    let expression = format!(
        "try!({name}Deserializer::deserialize(\"{location}\", stack))",
        name = member.shape,
        location = location_to_use,
    );

    if shape.required(member_name) {
        expression
    } else {
        format!("Some({})", expression)
    }
}

fn generate_serializer_body(shape: &Shape) -> String {
    match shape.shape_type {
        ShapeType::List => generate_list_serializer(shape),
        ShapeType::Map => generate_map_serializer(shape),
        ShapeType::Structure => generate_struct_serializer(shape),
        _ => generate_primitive_serializer(shape),
    }
}

fn generate_serializer_signature(name: &str, shape: &Shape) -> String {
    if shape.shape_type == ShapeType::Structure && shape.members.as_ref().unwrap().is_empty() {
        format!("
        	#[allow(unused_variables, warnings)]
        	fn serialize(name: &str, obj: &{}) -> String", name)
    } else {
        format!("
        	#[allow(unused_variables, warnings)]
        	fn serialize(name: &str, obj: &{}) -> String", name)
    }
}

fn generate_primitive_serializer(shape: &Shape) -> String {
	 let value_str = match shape.shape_type {
	 	ShapeType::Blob => "String::from_utf8(obj.to_vec()).expect(\"Not a UTF-8 string\")",
	 	_ => "obj.to_string()"
	 };
	 format!("format!(\"<{{name}}>{{value}}</{{name}}>\", 
			    name = name, 
				value = {value_str})",
				value_str = value_str)

}

fn generate_list_serializer(shape: &Shape) -> String {
	let member = shape.member.as_ref().unwrap();
	let element_type = &generate_member_name(shape);
	let location_name = match member.location_name {
		Some(ref name) => name,
		None => element_type
	};
    format!("
    	let mut parts: Vec<String> = Vec::new();
    	parts.push(format!(\"<{{}}>\", name));
    	for element in obj {{
    		parts.push({element_type}Serializer::serialize(\"{location_name}\", element));
    	}}
    	parts.push(format!(\"</{{}}>\", name));
    	parts.join(\"\\n\")
    	",
    	element_type = element_type,
    	location_name = location_name)
}

fn generate_map_serializer(_shape: &Shape) -> String {
    "String::new()".to_string()
}

fn generate_struct_serializer(_shape: &Shape) -> String {
   "String::new()".to_string()
}

