use proc_macro::TokenStream;
use quote::quote;
use raxis_core::PathCommand;
use syn::{LitStr, parse_macro_input};

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

            // Check if this is a command character or implicit continuation
            let command_char = if ch.is_alphabetic() {
                chars.next(); // consume command character
                last_command = Some(ch);
                ch
            } else {
                // Implicit continuation - use last command
                match last_command {
                    Some(cmd) => {
                        // Special case: after M, implicit coordinates become L
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
                    // Calculate reflected control point
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
                    // Calculate reflected control point
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
                    // Calculate reflected control point for smooth quadratic continuation
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
                    // Calculate reflected control point for smooth quadratic continuation
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
                _ => {
                    return Err(format!("Unknown command: {command_char}"));
                }
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

        // Handle negative sign
        if let Some(&'-') = chars.peek() {
            number_str.push(chars.next().unwrap());
        }

        // Parse digits and decimal point
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

/// Procedural macro to generate path geometry from SVG path strings
///
/// Usage:
/// ```rust
/// let path_func = svg_path!("M20 6 9 17l-5-5");
/// ```
///
/// This generates a function that creates a PathGeometry with the specified path.
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

    generate_path_geometry(&svg_path)
}

fn generate_path_geometry(svg_path: &SvgPathCommands) -> TokenStream {
    // Generate path commands as const array
    let command_tokens = svg_path.commands.iter().map(|cmd| match cmd {
        PathCommand::MoveTo { x, y } => {
            quote! { raxis_core::PathCommand::MoveTo { x: #x, y: #y } }
        }
        PathCommand::LineTo { x, y } => {
            quote! { raxis_core::PathCommand::LineTo { x: #x, y: #y } }
        }
        PathCommand::Arc {
            end_x,
            end_y,
            radius_x,
            radius_y,
            rotation,
            large_arc,
            sweep,
        } => {
            quote! { raxis_core::PathCommand::Arc {
                end_x: #end_x, end_y: #end_y,
                radius_x: #radius_x, radius_y: #radius_y,
                rotation: #rotation,
                large_arc: #large_arc, sweep: #sweep
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
            quote! { raxis_core::PathCommand::CubicBezier {
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
            quote! { raxis_core::PathCommand::QuadraticBezier {
                cp_x: #cp_x, cp_y: #cp_y,
                end_x: #end_x, end_y: #end_y
            } }
        }
        PathCommand::ClosePath => {
            quote! { raxis_core::PathCommand::ClosePath }
        }
    });

    let expanded = quote! {
        {
            const COMMANDS: &'static [raxis_core::PathCommand] = &[
                #(#command_tokens),*
            ];
            raxis_core::SvgPathCommands { commands: COMMANDS }
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
    fn test_dc() {
        let path = "M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z";
        let path = SvgPathCommands::parse(path).expect("Failed to parse path");
    }
}
