use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use raxis_core::PathCommand;
use syn::{
    Fields, Ident, ItemStruct, LitFloat, LitStr, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

// =============================================================================
// SVG Path Macro (unchanged)
// =============================================================================

#[derive(Debug)]
struct SvgPathCommands {
    pub commands: Vec<PathCommand>,
}

impl SvgPathCommands {
    fn parse(path_str: &str) -> Result<Self, String> {
        let mut commands = Vec::new();
        let mut chars = path_str.chars().peekable();
        let mut current_pos = (0.0f32, 0.0f32);
        let mut last_control_point: Option<(f32, f32)> = None;
        let mut last_command: Option<char> = None;

        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() {
                chars.next();
                continue;
            }

            let command_char = if ch.is_alphabetic() {
                chars.next();
                last_command = Some(ch);
                ch
            } else {
                match last_command {
                    Some(cmd) => {
                        if cmd == 'M' || cmd == 'm' {
                            if cmd == 'M' { 'L' } else { 'l' }
                        } else {
                            cmd
                        }
                    }
                    None => return Err("Path must start with a command".to_string()),
                }
            };

            match command_char {
                'M' => {
                    let (x, y) = Self::parse_coordinates(&mut chars)?;
                    current_pos = (x, y);
                    last_control_point = None;
                    commands.push(PathCommand::MoveTo { x, y });
                }
                'm' => {
                    let (dx, dy) = Self::parse_coordinates(&mut chars)?;
                    current_pos.0 += dx;
                    current_pos.1 += dy;
                    last_control_point = None;
                    commands.push(PathCommand::MoveTo {
                        x: current_pos.0,
                        y: current_pos.1,
                    });
                }
                'L' => {
                    let (x, y) = Self::parse_coordinates(&mut chars)?;
                    current_pos = (x, y);
                    last_control_point = None;
                    commands.push(PathCommand::LineTo { x, y });
                }
                'l' => {
                    let (dx, dy) = Self::parse_coordinates(&mut chars)?;
                    current_pos.0 += dx;
                    current_pos.1 += dy;
                    last_control_point = None;
                    commands.push(PathCommand::LineTo {
                        x: current_pos.0,
                        y: current_pos.1,
                    });
                }
                'H' => {
                    let x = Self::parse_number(&mut chars)?;
                    current_pos.0 = x;
                    last_control_point = None;
                    commands.push(PathCommand::LineTo {
                        x,
                        y: current_pos.1,
                    });
                }
                'h' => {
                    let dx = Self::parse_number(&mut chars)?;
                    current_pos.0 += dx;
                    last_control_point = None;
                    commands.push(PathCommand::LineTo {
                        x: current_pos.0,
                        y: current_pos.1,
                    });
                }
                'V' => {
                    let y = Self::parse_number(&mut chars)?;
                    current_pos.1 = y;
                    last_control_point = None;
                    commands.push(PathCommand::LineTo {
                        x: current_pos.0,
                        y,
                    });
                }
                'v' => {
                    let dy = Self::parse_number(&mut chars)?;
                    current_pos.1 += dy;
                    last_control_point = None;
                    commands.push(PathCommand::LineTo {
                        x: current_pos.0,
                        y: current_pos.1,
                    });
                }
                'C' => {
                    let (cp1_x, cp1_y) = Self::parse_coordinates(&mut chars)?;
                    let (cp2_x, cp2_y) = Self::parse_coordinates(&mut chars)?;
                    let (end_x, end_y) = Self::parse_coordinates(&mut chars)?;
                    current_pos = (end_x, end_y);
                    last_control_point = Some((cp2_x, cp2_y));
                    commands.push(PathCommand::CubicBezier {
                        cp1_x,
                        cp1_y,
                        cp2_x,
                        cp2_y,
                        end_x,
                        end_y,
                    });
                }
                'c' => {
                    let (dcp1_x, dcp1_y) = Self::parse_coordinates(&mut chars)?;
                    let (dcp2_x, dcp2_y) = Self::parse_coordinates(&mut chars)?;
                    let (dend_x, dend_y) = Self::parse_coordinates(&mut chars)?;
                    let cp1_x = current_pos.0 + dcp1_x;
                    let cp1_y = current_pos.1 + dcp1_y;
                    let cp2_x = current_pos.0 + dcp2_x;
                    let cp2_y = current_pos.1 + dcp2_y;
                    current_pos.0 += dend_x;
                    current_pos.1 += dend_y;
                    last_control_point = Some((cp2_x, cp2_y));
                    commands.push(PathCommand::CubicBezier {
                        cp1_x,
                        cp1_y,
                        cp2_x,
                        cp2_y,
                        end_x: current_pos.0,
                        end_y: current_pos.1,
                    });
                }
                'S' => {
                    let (cp2_x, cp2_y) = Self::parse_coordinates(&mut chars)?;
                    let (end_x, end_y) = Self::parse_coordinates(&mut chars)?;
                    let (cp1_x, cp1_y) = if let Some((last_cp_x, last_cp_y)) = last_control_point {
                        (
                            2.0 * current_pos.0 - last_cp_x,
                            2.0 * current_pos.1 - last_cp_y,
                        )
                    } else {
                        current_pos
                    };
                    current_pos = (end_x, end_y);
                    last_control_point = Some((cp2_x, cp2_y));
                    commands.push(PathCommand::CubicBezier {
                        cp1_x,
                        cp1_y,
                        cp2_x,
                        cp2_y,
                        end_x,
                        end_y,
                    });
                }
                's' => {
                    let (dcp2_x, dcp2_y) = Self::parse_coordinates(&mut chars)?;
                    let (dend_x, dend_y) = Self::parse_coordinates(&mut chars)?;
                    let cp2_x = current_pos.0 + dcp2_x;
                    let cp2_y = current_pos.1 + dcp2_y;
                    let (cp1_x, cp1_y) = if let Some((last_cp_x, last_cp_y)) = last_control_point {
                        (
                            2.0 * current_pos.0 - last_cp_x,
                            2.0 * current_pos.1 - last_cp_y,
                        )
                    } else {
                        current_pos
                    };
                    current_pos.0 += dend_x;
                    current_pos.1 += dend_y;
                    last_control_point = Some((cp2_x, cp2_y));
                    commands.push(PathCommand::CubicBezier {
                        cp1_x,
                        cp1_y,
                        cp2_x,
                        cp2_y,
                        end_x: current_pos.0,
                        end_y: current_pos.1,
                    });
                }
                'Q' => {
                    let (cp_x, cp_y) = Self::parse_coordinates(&mut chars)?;
                    let (end_x, end_y) = Self::parse_coordinates(&mut chars)?;
                    current_pos = (end_x, end_y);
                    last_control_point = Some((cp_x, cp_y));
                    commands.push(PathCommand::QuadraticBezier {
                        cp_x,
                        cp_y,
                        end_x,
                        end_y,
                    });
                }
                'q' => {
                    let (dcp_x, dcp_y) = Self::parse_coordinates(&mut chars)?;
                    let (dend_x, dend_y) = Self::parse_coordinates(&mut chars)?;
                    let cp_x = current_pos.0 + dcp_x;
                    let cp_y = current_pos.1 + dcp_y;
                    current_pos.0 += dend_x;
                    current_pos.1 += dend_y;
                    last_control_point = Some((cp_x, cp_y));
                    commands.push(PathCommand::QuadraticBezier {
                        cp_x,
                        cp_y,
                        end_x: current_pos.0,
                        end_y: current_pos.1,
                    });
                }
                'T' => {
                    let (end_x, end_y) = Self::parse_coordinates(&mut chars)?;
                    let (cp_x, cp_y) = if let Some((last_cp_x, last_cp_y)) = last_control_point {
                        (
                            2.0 * current_pos.0 - last_cp_x,
                            2.0 * current_pos.1 - last_cp_y,
                        )
                    } else {
                        current_pos
                    };
                    current_pos = (end_x, end_y);
                    last_control_point = Some((cp_x, cp_y));
                    commands.push(PathCommand::QuadraticBezier {
                        cp_x,
                        cp_y,
                        end_x,
                        end_y,
                    });
                }
                't' => {
                    let (dend_x, dend_y) = Self::parse_coordinates(&mut chars)?;
                    let (cp_x, cp_y) = if let Some((last_cp_x, last_cp_y)) = last_control_point {
                        (
                            2.0 * current_pos.0 - last_cp_x,
                            2.0 * current_pos.1 - last_cp_y,
                        )
                    } else {
                        current_pos
                    };
                    current_pos.0 += dend_x;
                    current_pos.1 += dend_y;
                    last_control_point = Some((cp_x, cp_y));
                    commands.push(PathCommand::QuadraticBezier {
                        cp_x,
                        cp_y,
                        end_x: current_pos.0,
                        end_y: current_pos.1,
                    });
                }
                'A' => {
                    let (radius_x, radius_y) = Self::parse_coordinates(&mut chars)?;
                    let rotation = Self::parse_number(&mut chars)?;
                    let large_arc = Self::parse_flag(&mut chars)?;
                    let sweep = Self::parse_flag(&mut chars)?;
                    let (end_x, end_y) = Self::parse_coordinates(&mut chars)?;
                    current_pos = (end_x, end_y);
                    last_control_point = None;
                    commands.push(PathCommand::Arc {
                        end_x,
                        end_y,
                        radius_x,
                        radius_y,
                        rotation,
                        large_arc,
                        sweep,
                    });
                }
                'a' => {
                    let (radius_x, radius_y) = Self::parse_coordinates(&mut chars)?;
                    let rotation = Self::parse_number(&mut chars)?;
                    let large_arc = Self::parse_flag(&mut chars)?;
                    let sweep = Self::parse_flag(&mut chars)?;
                    let (dend_x, dend_y) = Self::parse_coordinates(&mut chars)?;
                    current_pos.0 += dend_x;
                    current_pos.1 += dend_y;
                    last_control_point = None;
                    commands.push(PathCommand::Arc {
                        end_x: current_pos.0,
                        end_y: current_pos.1,
                        radius_x,
                        radius_y,
                        rotation,
                        large_arc,
                        sweep,
                    });
                }
                'Z' | 'z' => {
                    last_control_point = None;
                    commands.push(PathCommand::ClosePath);
                }
                _ => return Err(format!("Unknown command: {command_char}")),
            }
        }

        Ok(SvgPathCommands { commands })
    }

    fn parse_coordinates(
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<(f32, f32), String> {
        let x = Self::parse_number(chars)?;
        Self::skip_separators(chars);
        let y = Self::parse_number(chars)?;
        Ok((x, y))
    }

    pub(crate) fn parse_number(
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<f32, String> {
        Self::skip_separators(chars);
        let mut number_str = String::new();
        if let Some(&'-') = chars.peek() {
            number_str.push(chars.next().unwrap());
        }
        let mut seen_decimal = false;
        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() || ch == '.' {
                if ch == '.' && seen_decimal {
                    break;
                }
                number_str.push(chars.next().unwrap());
                seen_decimal = seen_decimal || ch == '.';
            } else {
                break;
            }
        }
        number_str
            .parse::<f32>()
            .map_err(|_| format!("Invalid number: {number_str}"))
    }

    fn parse_flag(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<bool, String> {
        Self::skip_separators(chars);
        if let Some(&ch) = chars.peek() {
            if ch == '0' {
                chars.next();
                Ok(false)
            } else if ch == '1' {
                chars.next();
                Ok(true)
            } else {
                Err(format!("Invalid flag value: {ch}"))
            }
        } else {
            Err("Expected flag value (0 or 1)".to_string())
        }
    }

    fn skip_separators(chars: &mut std::iter::Peekable<std::str::Chars>) {
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() || ch == ',' {
                chars.next();
            } else {
                break;
            }
        }
    }
}

#[proc_macro]
pub fn svg_path(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let path_str = input.value();

    let svg_path = match SvgPathCommands::parse(&path_str) {
        Ok(path) => path,
        Err(e) => {
            return syn::Error::new_spanned(&input, format!("Failed to parse SVG path: {e}"))
                .to_compile_error()
                .into();
        }
    };

    let command_tokens = svg_path.commands.iter().map(|cmd| match cmd {
        PathCommand::MoveTo { x, y } => quote! { raxis::PathCommand::MoveTo { x: #x, y: #y } },
        PathCommand::LineTo { x, y } => quote! { raxis::PathCommand::LineTo { x: #x, y: #y } },
        PathCommand::Arc {
            end_x,
            end_y,
            radius_x,
            radius_y,
            rotation,
            large_arc,
            sweep,
        } => {
            quote! { raxis::PathCommand::Arc {
                end_x: #end_x, end_y: #end_y,
                radius_x: #radius_x, radius_y: #radius_y,
                rotation: #rotation, large_arc: #large_arc, sweep: #sweep
            } }
        }
        PathCommand::CubicBezier {
            cp1_x,
            cp1_y,
            cp2_x,
            cp2_y,
            end_x,
            end_y,
        } => {
            quote! { raxis::PathCommand::CubicBezier {
                cp1_x: #cp1_x, cp1_y: #cp1_y,
                cp2_x: #cp2_x, cp2_y: #cp2_y,
                end_x: #end_x, end_y: #end_y
            } }
        }
        PathCommand::QuadraticBezier {
            cp_x,
            cp_y,
            end_x,
            end_y,
        } => {
            quote! { raxis::PathCommand::QuadraticBezier {
                cp_x: #cp_x, cp_y: #cp_y,
                end_x: #end_x, end_y: #end_y
            } }
        }
        PathCommand::ClosePath => quote! { raxis::PathCommand::ClosePath },
    });

    let expanded = quote! {{
        const COMMANDS: &'static [raxis::PathCommand] = &[ #(#command_tokens),* ];
        raxis::SvgPathCommands::Path(COMMANDS)
    }};

    TokenStream::from(expanded)
}

// =============================================================================
// Pixel Shader Effect Macro
// =============================================================================

/// Parsed effect-level attributes
struct EffectAttrs {
    clsid: String,
    inputs: u32,
    name: String,
    author: String,
    category: String,
    description: String,
    shader: String,
    /// Optional field name to use for input_padding (for effects that sample neighbors)
    input_padding_field: Option<String>,
}

impl Parse for EffectAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut clsid = None;
        let mut inputs = 1u32;
        let mut name = None;
        let mut author = None;
        let mut category = None;
        let mut description = None;
        let mut shader = None;
        let mut input_padding_field = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "clsid" => {
                    let value: LitStr = input.parse()?;
                    clsid = Some(value.value());
                }
                "inputs" => {
                    let value: syn::LitInt = input.parse()?;
                    inputs = value.base10_parse()?;
                }
                "name" => {
                    let value: LitStr = input.parse()?;
                    name = Some(value.value());
                }
                "author" => {
                    let value: LitStr = input.parse()?;
                    author = Some(value.value());
                }
                "category" => {
                    let value: LitStr = input.parse()?;
                    category = Some(value.value());
                }
                "description" => {
                    let value: LitStr = input.parse()?;
                    description = Some(value.value());
                }
                "shader" => {
                    let value: LitStr = input.parse()?;
                    shader = Some(value.value());
                }
                "input_padding" => {
                    let value: LitStr = input.parse()?;
                    input_padding_field = Some(value.value());
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("Unknown attribute: {}", key),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(EffectAttrs {
            clsid: clsid.ok_or_else(|| input.error("Missing required attribute: clsid"))?,
            inputs,
            name: name.ok_or_else(|| input.error("Missing required attribute: name"))?,
            author: author.unwrap_or_else(|| "Unknown".to_string()),
            category: category.unwrap_or_else(|| "Custom".to_string()),
            description: description.unwrap_or_else(|| "".to_string()),
            shader: shader.ok_or_else(|| input.error("Missing required attribute: shader"))?,
            input_padding_field,
        })
    }
}

/// Parsed property attributes from #[property(...)]
#[derive(Default)]
struct PropertyAttrs {
    name: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    default: Option<f64>,
}

impl PropertyAttrs {
    fn parse_from_attrs(attrs: &[syn::Attribute]) -> Option<Self> {
        for attr in attrs {
            if attr.path().is_ident("property") {
                let mut result = PropertyAttrs::default();

                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("name") {
                        let value: LitStr = meta.value()?.parse()?;
                        result.name = Some(value.value());
                    } else if meta.path.is_ident("min") {
                        let value: LitFloat = meta.value()?.parse()?;
                        result.min = Some(value.base10_parse()?);
                    } else if meta.path.is_ident("max") {
                        let value: LitFloat = meta.value()?.parse()?;
                        result.max = Some(value.base10_parse()?);
                    } else if meta.path.is_ident("default") {
                        let value: LitFloat = meta.value()?.parse()?;
                        result.default = Some(value.base10_parse()?);
                    }
                    Ok(())
                });

                return Some(result);
            }
        }
        None
    }
}

/// Property type info extracted from field
struct PropertyInfo {
    field_name: Ident,
    field_type: Type,
    prop_name: String,
    min: Option<f64>,
    max: Option<f64>,
    default: f64,
    d2d_type: &'static str,
    size_bytes: usize,
}

impl PropertyInfo {
    fn from_field(field: &syn::Field) -> Option<Self> {
        let field_name = field.ident.clone()?;
        let field_type = field.ty.clone();
        let attrs = PropertyAttrs::parse_from_attrs(&field.attrs).unwrap_or_default();

        // Determine D2D type and size from Rust type
        let type_str = quote!(#field_type).to_string();
        let (d2d_type, size_bytes, default_val) = match type_str.as_str() {
            "f32" => ("float", 4, 0.0),
            "i32" => ("int32", 4, 0.0),
            "u32" => ("uint32", 4, 0.0),
            "bool" => ("bool", 4, 0.0),
            "[f32 ; 2]" | "[f32; 2]" => ("vector2", 8, 0.0),
            "[f32 ; 3]" | "[f32; 3]" => ("vector3", 12, 0.0),
            "[f32 ; 4]" | "[f32; 4]" => ("vector4", 16, 0.0),
            _ => return None, // Skip unknown types
        };

        let prop_name = attrs.name.unwrap_or_else(|| {
            // Convert snake_case to PascalCase for display name
            field_name
                .to_string()
                .split('_')
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect()
        });

        Some(PropertyInfo {
            field_name,
            field_type,
            prop_name,
            min: attrs.min,
            max: attrs.max,
            default: attrs.default.unwrap_or(default_val),
            d2d_type,
            size_bytes,
        })
    }
}

/// Parse CLSID string to u128 for GUID::from_u128
fn parse_clsid(clsid: &str) -> Result<u128, String> {
    // Remove hyphens and parse as hex
    let clean: String = clsid.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() != 32 {
        return Err(format!(
            "Invalid CLSID format: expected 32 hex chars, got {}",
            clean.len()
        ));
    }
    u128::from_str_radix(&clean, 16).map_err(|e| format!("Invalid CLSID: {}", e))
}

/// Generate the PixelShaderEffect trait implementation
fn generate_trait_impl(
    name: &Ident,
    attrs: &EffectAttrs,
    properties: &[PropertyInfo],
) -> TokenStream2 {
    let clsid = parse_clsid(&attrs.clsid).expect("Invalid CLSID format");
    let clsid_hex = format!("0x{:032X}", clsid);
    let clsid_lit: syn::LitInt = syn::parse_str(&clsid_hex).unwrap();

    let input_count = attrs.inputs;
    let effect_name = &attrs.name;
    let effect_author = &attrs.author;
    let effect_category = &attrs.category;
    let effect_description = &attrs.description;
    let shader_path = &attrs.shader;

    // Generate PropertyMetadata entries
    let property_metadata = properties.iter().map(|p| {
        let prop_name = &p.prop_name;
        let d2d_type = match p.d2d_type {
            "float" => quote!(::raxis::gfx::effects::PropertyType::Float),
            "int32" => quote!(::raxis::gfx::effects::PropertyType::Int),
            "uint32" => quote!(::raxis::gfx::effects::PropertyType::UInt),
            "bool" => quote!(::raxis::gfx::effects::PropertyType::Bool),
            "vector2" => quote!(::raxis::gfx::effects::PropertyType::Vector2),
            "vector3" => quote!(::raxis::gfx::effects::PropertyType::Vector3),
            "vector4" => quote!(::raxis::gfx::effects::PropertyType::Vector4),
            _ => quote!(::raxis::gfx::effects::PropertyType::Float),
        };
        let default_val = p.default as f32;
        let default_expr = quote!(::raxis::gfx::effects::PropertyDefault::Float(#default_val));

        let min_expr = if let Some(min) = p.min {
            let min = min as f32;
            quote!(Some(::raxis::gfx::effects::PropertyDefault::Float(#min)))
        } else {
            quote!(None)
        };

        let max_expr = if let Some(max) = p.max {
            let max = max as f32;
            quote!(Some(::raxis::gfx::effects::PropertyDefault::Float(#max)))
        } else {
            quote!(None)
        };

        quote! {
            ::raxis::gfx::effects::PropertyMetadata {
                name: #prop_name,
                display_name: #prop_name,
                property_type: #d2d_type,
                default: #default_expr,
                min: #min_expr,
                max: #max_expr,
            }
        }
    });

    // Generate EffectProperty entries from fields
    let property_values = properties.iter().enumerate().map(|(i, p)| {
        let field_name = &p.field_name;
        let idx = i as u32;

        match p.d2d_type {
            "float" => quote! {
                ::raxis::gfx::effects::EffectProperty::Float { index: #idx, value: self.#field_name }
            },
            "int32" => quote! {
                ::raxis::gfx::effects::EffectProperty::Int { index: #idx, value: self.#field_name }
            },
            "uint32" => quote! {
                ::raxis::gfx::effects::EffectProperty::UInt { index: #idx, value: self.#field_name }
            },
            "bool" => quote! {
                ::raxis::gfx::effects::EffectProperty::Bool { index: #idx, value: self.#field_name }
            },
            "vector2" => quote! {
                ::raxis::gfx::effects::EffectProperty::Float2 { index: #idx, value: self.#field_name }
            },
            "vector3" => quote! {
                ::raxis::gfx::effects::EffectProperty::Float3 { index: #idx, value: self.#field_name }
            },
            "vector4" => quote! {
                ::raxis::gfx::effects::EffectProperty::Float4 { index: #idx, value: self.#field_name }
            },
            _ => quote! {
                ::raxis::gfx::effects::EffectProperty::Float { index: #idx, value: self.#field_name as f32 }
            },
        }
    });

    // Generate input_padding method if an expression is specified
    let input_padding_impl = if let Some(expr_str) = &attrs.input_padding_field {
        // Parse the expression string as a Rust expression
        let expr: syn::Expr = syn::parse_str(expr_str).expect(&format!(
            "Failed to parse input_padding expression: {}",
            expr_str
        ));
        quote! {
            fn input_padding(&self) -> f32 {
                #expr
            }
        }
    } else {
        quote! {}
    };

    quote! {
        impl ::raxis::gfx::effects::PixelShaderEffect for #name {
            const CLSID: ::windows::core::GUID = ::windows::core::GUID::from_u128(#clsid_lit);
            const INPUT_COUNT: u32 = #input_count;

            fn metadata() -> ::raxis::gfx::effects::EffectMetadata {
                ::raxis::gfx::effects::EffectMetadata {
                    name: #effect_name,
                    author: #effect_author,
                    category: #effect_category,
                    description: #effect_description,
                    shader_bytecode: include_bytes!(#shader_path),
                    properties: &[ #(#property_metadata),* ],
                }
            }

            fn properties(&self) -> Vec<::raxis::gfx::effects::EffectProperty> {
                vec![ #(#property_values),* ]
            }

            #input_padding_impl
        }
    }
}

/// Generate the COM wrapper and property bindings
fn generate_com_wrapper(name: &Ident, properties: &[PropertyInfo]) -> TokenStream2 {
    // Generate property storage fields using UnsafeCell for simplicity
    let property_fields = properties.iter().map(|p| {
        let field_name = &p.field_name;
        let field_type = &p.field_type;
        quote! { #field_name: UnsafeCell<#field_type> }
    });

    // Generate property field initialization with defaults
    let property_inits = properties.iter().map(|p| {
        let field_name = &p.field_name;
        let default = p.default as f32;
        let type_str = p.d2d_type;

        match type_str {
            "float" => quote! { #field_name: UnsafeCell::new(#default) },
            "int32" => {
                let default = p.default as i32;
                quote! { #field_name: UnsafeCell::new(#default) }
            }
            "uint32" => {
                let default = p.default as u32;
                quote! { #field_name: UnsafeCell::new(#default) }
            }
            "bool" => {
                let default = p.default != 0.0;
                quote! { #field_name: UnsafeCell::new(#default) }
            }
            "vector2" => quote! { #field_name: UnsafeCell::new([0.0f32, 0.0f32]) },
            "vector3" => quote! { #field_name: UnsafeCell::new([0.0f32, 0.0f32, 0.0f32]) },
            "vector4" => quote! { #field_name: UnsafeCell::new([0.0f32, 0.0f32, 0.0f32, 0.0f32]) },
            _ => quote! { #field_name: UnsafeCell::new(#default) },
        }
    });

    // Generate constant buffer fields
    let cb_fields = properties.iter().map(|p| {
        let field_name = &p.field_name;
        let field_type = &p.field_type;
        quote! { #field_name: #field_type }
    });

    // Generate constant buffer population
    let cb_populate = properties.iter().map(|p| {
        let field_name = &p.field_name;
        quote! { #field_name: unsafe { *self.#field_name.get() } }
    });

    // Generate getter/setter function names and implementations
    // Note: The D2D property functions receive an IUnknown pointer to the effect
    let getters_setters: Vec<_> = properties
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let field_name = &p.field_name;
            let getter_name = format_ident!("get_prop_{}", i);
            let setter_name = format_ident!("set_prop_{}", i);
            let size = p.size_bytes;

            // Getter function matches PD2D1_PROPERTY_GET_FUNCTION signature
            let getter = quote! {
                unsafe extern "system" fn #getter_name(
                    effect: Ref<'_, IUnknown>,
                    data: *mut u8,
                    data_size: u32,
                    actual_size: *mut u32,
                ) -> HRESULT {
                    if data.is_null() {
                        if !actual_size.is_null() {
                            *actual_size = #size as u32;
                        }
                        return S_OK;
                    }

                    if data_size < #size as u32 {
                        return E_INVALIDARG;
                    }

                    // Get the EffectImpl from the IUnknown pointer
                    // The effect pointer is actually our EffectImpl wrapped in COM
                    let effect_ptr = effect.as_ref().unwrap();
                    let effect_impl = effect_ptr.cast::<ID2D1EffectImpl>().unwrap();
                    let effect_impl = AsImpl::<EffectImpl>::as_impl(&effect_impl);

                    let value = *(*effect_impl).#field_name.get();
                    ::std::ptr::copy_nonoverlapping(
                        &value as *const _ as *const u8,
                        data,
                        #size,
                    );

                    if !actual_size.is_null() {
                        *actual_size = #size as u32;
                    }

                    S_OK
                }
            };

            // Setter function matches PD2D1_PROPERTY_SET_FUNCTION signature
            let setter = quote! {
                unsafe extern "system" fn #setter_name(
                    effect: Ref<'_, IUnknown>,
                    data: *const u8,
                    data_size: u32,
                ) -> HRESULT {
                    if data.is_null() || data_size < #size as u32 {
                        return E_INVALIDARG;
                    }

                    // Get the EffectImpl from the IUnknown pointer
                    let effect_ptr = effect.as_ref().unwrap();
                    let effect_impl = effect_ptr.cast::<ID2D1EffectImpl>().unwrap();
                    let effect_impl = AsImpl::<EffectImpl>::as_impl(&effect_impl);

                    let ptr = (*effect_impl).#field_name.get();
                    ::std::ptr::copy_nonoverlapping(
                        data,
                        ptr as *mut u8,
                        #size,
                    );

                    S_OK
                }
            };

            (getter, setter, getter_name, setter_name)
        })
        .collect();

    let getter_fns = getters_setters.iter().map(|(g, _, _, _)| g);
    let setter_fns = getters_setters.iter().map(|(_, s, _, _)| s);

    // Generate property bindings wrapped in SyncPropertyBinding
    let bindings = properties.iter().enumerate().map(|(i, p)| {
        let prop_name = &p.prop_name;
        let getter_name = format_ident!("get_prop_{}", i);
        let setter_name = format_ident!("set_prop_{}", i);

        quote! {
            ::raxis::gfx::effects::SyncPropertyBinding::new(
                D2D1_PROPERTY_BINDING {
                    propertyName: w!(#prop_name),
                    setFunction: Some(#setter_name),
                    getFunction: Some(#getter_name),
                }
            )
        }
    });

    quote! {
        const _: () = {
            use ::windows::Win32::Graphics::Direct2D::{
                ID2D1EffectImpl, ID2D1EffectImpl_Impl,
                ID2D1DrawTransform, ID2D1DrawTransform_Impl,
                ID2D1Transform, ID2D1Transform_Impl,
                ID2D1TransformNode, ID2D1TransformNode_Impl,
                ID2D1EffectContext, ID2D1TransformGraph, ID2D1DrawInfo,
                D2D1_CHANGE_TYPE, D2D1_PIXEL_OPTIONS_NONE,
                D2D1_PROPERTY_BINDING,
            };
            use ::windows::Win32::Foundation::{RECT, E_NOTIMPL, E_INVALIDARG, S_OK};
            use ::windows::core::{GUID, HRESULT, Error, implement, w};
            use ::windows_core::{Ref, OutRef, AsImpl, IUnknown, Interface};
            use ::std::cell::{RefCell, UnsafeCell};
            use ::std::sync::Arc;
            use ::std::sync::RwLock;

            // Shared state between EffectImpl and TransformImpl
            type SharedDrawInfo = Arc<RwLock<Option<ID2D1DrawInfo>>>;

            // Constant buffer struct (matches HLSL layout)
            #[repr(C)]
            #[derive(Clone, Copy, Debug)]
            struct ConstantBuffer {
                #(#cb_fields),*
            }

            // Generated COM wrapper implementing ID2D1EffectImpl
            #[implement(ID2D1EffectImpl)]
            struct EffectImpl {
                effect_context: RefCell<Option<ID2D1EffectContext>>,
                draw_info: SharedDrawInfo,
                // Property storage
                #(#property_fields),*
            }

            impl EffectImpl {
                fn new() -> Self {
                    Self {
                        effect_context: RefCell::new(None),
                        draw_info: Arc::new(RwLock::new(None)),
                        #(#property_inits),*
                    }
                }

                fn build_constant_buffer(&self) -> ConstantBuffer {
                    ConstantBuffer {
                        #(#cb_populate),*
                    }
                }
            }

            // Getter/setter functions for property bindings
            #(#getter_fns)*
            #(#setter_fns)*

            impl ID2D1EffectImpl_Impl for EffectImpl_Impl {
                fn Initialize(
                    &self,
                    effectcontext: Ref<'_, ID2D1EffectContext>,
                    transformgraph: Ref<'_, ID2D1TransformGraph>,
                ) -> ::windows::core::Result<()> {
                    let context: &ID2D1EffectContext = effectcontext
                        .as_ref()
                        .ok_or(Error::from(E_NOTIMPL))?;
                    let graph: &ID2D1TransformGraph = transformgraph
                        .as_ref()
                        .ok_or(Error::from(E_NOTIMPL))?;

                    *self.effect_context.borrow_mut() = Some(context.clone());

                    let metadata = <#name as ::raxis::gfx::effects::PixelShaderEffect>::metadata();
                    let clsid = <#name as ::raxis::gfx::effects::PixelShaderEffect>::CLSID;

                    unsafe {
                        context.LoadPixelShader(&clsid, metadata.shader_bytecode)?;
                    }

                    // Create transform with shared draw_info
                    let transform_node = TransformImpl::create(self.draw_info.clone());

                    unsafe {
                        graph.AddNode(&transform_node)?;
                        graph.SetSingleTransformNode(&transform_node)?;
                    }

                    Ok(())
                }

                fn PrepareForRender(&self, _changetype: D2D1_CHANGE_TYPE) -> ::windows::core::Result<()> {
                    // Build constant buffer from stored properties
                    let cb = self.build_constant_buffer();

                    // Get draw_info from shared state and set constant buffer
                    if let Ok(guard) = self.draw_info.read() {
                        if let Some(ref draw_info) = *guard {
                            unsafe {
                                let cb_bytes = ::std::slice::from_raw_parts(
                                    &cb as *const ConstantBuffer as *const u8,
                                    ::std::mem::size_of::<ConstantBuffer>(),
                                );
                                draw_info.SetPixelShaderConstantBuffer(cb_bytes)?;
                            }
                        }
                    }

                    Ok(())
                }

                fn SetGraph(&self, _transformgraph: Ref<'_, ID2D1TransformGraph>) -> ::windows::core::Result<()> {
                    Err(Error::from(E_NOTIMPL))
                }
            }

            // Transform implementation
            #[implement(ID2D1DrawTransform, ID2D1Transform, ID2D1TransformNode)]
            struct TransformImpl {
                shared_draw_info: SharedDrawInfo,
                input_rect: RefCell<RECT>,
            }

            impl TransformImpl {
                fn create(shared_draw_info: SharedDrawInfo) -> ID2D1TransformNode {
                    let transform = TransformImpl {
                        shared_draw_info,
                        input_rect: RefCell::new(RECT::default()),
                    };
                    transform.into()
                }

                #[inline]
                fn input_count() -> u32 {
                    <#name as ::raxis::gfx::effects::PixelShaderEffect>::INPUT_COUNT
                }

                #[inline]
                fn clsid() -> GUID {
                    <#name as ::raxis::gfx::effects::PixelShaderEffect>::CLSID
                }
            }

            impl ID2D1TransformNode_Impl for TransformImpl_Impl {
                fn GetInputCount(&self) -> u32 {
                    TransformImpl::input_count()
                }
            }

            impl ID2D1Transform_Impl for TransformImpl_Impl {
                fn MapOutputRectToInputRects(
                    &self,
                    outputrect: *const RECT,
                    inputrects: *mut RECT,
                    inputrectscount: u32,
                ) -> ::windows::core::Result<()> {
                    if inputrectscount != TransformImpl::input_count() {
                        return Err(Error::from(E_INVALIDARG));
                    }
                    unsafe {
                        for i in 0..inputrectscount {
                            *inputrects.add(i as usize) = *outputrect;
                        }
                    }
                    Ok(())
                }

                fn MapInputRectsToOutputRect(
                    &self,
                    inputrects: *const RECT,
                    _inputopaquesubrects: *const RECT,
                    inputrectcount: u32,
                    outputrect: *mut RECT,
                    outputopaquesubrect: *mut RECT,
                ) -> ::windows::core::Result<()> {
                    if inputrectcount != TransformImpl::input_count() {
                        return Err(Error::from(E_INVALIDARG));
                    }
                    unsafe {
                        *self.input_rect.borrow_mut() = *inputrects;
                        *outputrect = *inputrects;
                        *outputopaquesubrect = RECT::default();
                    }
                    Ok(())
                }

                fn MapInvalidRect(
                    &self,
                    inputindex: u32,
                    invalidinputrect: &RECT,
                ) -> ::windows::core::Result<RECT> {
                    if inputindex >= TransformImpl::input_count() {
                        return Err(Error::from(E_INVALIDARG));
                    }
                    Ok(*invalidinputrect)
                }
            }

            impl ID2D1DrawTransform_Impl for TransformImpl_Impl {
                fn SetDrawInfo(&self, drawinfo: Ref<'_, ID2D1DrawInfo>) -> ::windows::core::Result<()> {
                    let info: &ID2D1DrawInfo = drawinfo
                        .as_ref()
                        .ok_or(Error::from(E_NOTIMPL))?;

                    unsafe {
                        info.SetPixelShader(&TransformImpl::clsid(), D2D1_PIXEL_OPTIONS_NONE)?;
                    }

                    // Store draw_info in shared state so EffectImpl can access it
                    if let Ok(mut guard) = self.shared_draw_info.write() {
                        *guard = Some(info.clone());
                    }
                    Ok(())
                }
            }

            // Property bindings array (wrapped for Sync safety)
            static PROPERTY_BINDINGS: &[::raxis::gfx::effects::SyncPropertyBinding] = &[ #(#bindings),* ];

            // Factory function
            unsafe extern "system" fn effect_factory_fn(
                effect_impl: OutRef<'_, IUnknown>,
            ) -> HRESULT {
                let wrapper = EffectImpl::new();
                let effect: ID2D1EffectImpl = wrapper.into();

                if let Ok(unknown) = effect.cast::<IUnknown>() {
                    effect_impl.write(Some(unknown)).ok();
                    S_OK
                } else {
                    E_NOTIMPL
                }
            }

            impl ::raxis::gfx::effects::EffectFactory for #name {
                fn effect_factory() -> unsafe extern "system" fn(OutRef<'_, IUnknown>) -> HRESULT {
                    effect_factory_fn
                }

                fn property_bindings() -> &'static [::raxis::gfx::effects::SyncPropertyBinding] {
                    PROPERTY_BINDINGS
                }
            }
        };
    }
}

/// Attribute macro for defining pixel shader effects with full COM integration.
///
/// # Example
///
/// ```ignore
/// #[pixel_shader_effect(
///     clsid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
///     name = "Grayscale",
///     author = "raxis",
///     category = "Color",
///     description = "Converts image to grayscale",
///     shader = "shaders/grayscale.cso",
/// )]
/// #[derive(Debug, Clone, Copy)]
/// pub struct GrayscaleEffect {
///     #[property(min = 0.0, max = 1.0, default = 1.0)]
///     pub intensity: f32,
/// }
/// ```
#[proc_macro_attribute]
pub fn pixel_shader_effect(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs = parse_macro_input!(attr as EffectAttrs);
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;

    // Extract properties from struct fields
    let properties: Vec<PropertyInfo> = match &input.fields {
        Fields::Named(fields) => fields
            .named
            .iter()
            .filter_map(PropertyInfo::from_field)
            .collect(),
        _ => Vec::new(),
    };

    // Filter out #[property] attributes from the struct definition
    let mut clean_input = input.clone();
    if let Fields::Named(ref mut fields) = clean_input.fields {
        for field in fields.named.iter_mut() {
            field.attrs.retain(|attr| !attr.path().is_ident("property"));
        }
    }

    // Generate the trait implementation
    let trait_impl = generate_trait_impl(name, &attrs, &properties);

    // Generate the COM wrapper
    let com_wrapper = generate_com_wrapper(name, &properties);

    let expanded = quote! {
        #clean_input

        #trait_impl

        #com_wrapper

        impl #name {
            #[doc(hidden)]
            pub fn effect_factory() -> unsafe extern "system" fn(
                ::windows_core::OutRef<'_, ::windows_core::IUnknown>,
            ) -> ::windows::core::HRESULT {
                <Self as ::raxis::gfx::effects::EffectFactory>::effect_factory()
            }
        }
    };

    TokenStream::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_number() {
        let mut chars = "123.456.123".chars().peekable();
        let result = SvgPathCommands::parse_number(&mut chars);
        assert_eq!(result, Ok(123.456));
    }

    #[test]
    fn test_parse_clsid() {
        let clsid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";
        let result = parse_clsid(clsid);
        assert!(result.is_ok());
    }
}
