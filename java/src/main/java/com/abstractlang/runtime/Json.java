package com.abstractlang.runtime;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Minimal JSON parser for compiled Abstract documents. Produces
 * {@link LinkedHashMap} for objects (field order preserved),
 * {@link ArrayList} for arrays, {@link String}, {@link Long},
 * {@link Double}, {@link Boolean}, and {@code null}.
 */
final class Json {

    private final String text;
    private int position;

    private Json(String text) {
        this.text = text;
    }

    static Object parse(String text) {
        Json parser = new Json(text);
        parser.skipWhitespace();
        Object value = parser.readValue();
        parser.skipWhitespace();
        if (parser.position < parser.text.length()) {
            throw parser.error("unexpected trailing content");
        }
        return value;
    }

    private Object readValue() {
        if (position >= text.length()) {
            throw error("unexpected end of document");
        }
        char head = text.charAt(position);
        switch (head) {
            case '{':
                return readObject();
            case '[':
                return readArray();
            case '"':
                return readString();
            case 't':
                expect("true");
                return Boolean.TRUE;
            case 'f':
                expect("false");
                return Boolean.FALSE;
            case 'n':
                expect("null");
                return null;
            default:
                return readNumber();
        }
    }

    private Map<String, Object> readObject() {
        Map<String, Object> object = new LinkedHashMap<String, Object>();
        position++; // consume '{'
        skipWhitespace();
        if (peek() == '}') {
            position++;
            return object;
        }
        while (true) {
            skipWhitespace();
            if (peek() != '"') {
                throw error("expected an object key");
            }
            String key = readString();
            skipWhitespace();
            if (peek() != ':') {
                throw error("expected ':' after object key");
            }
            position++;
            skipWhitespace();
            object.put(key, readValue());
            skipWhitespace();
            char next = peek();
            if (next == ',') {
                position++;
                continue;
            }
            if (next == '}') {
                position++;
                return object;
            }
            throw error("expected ',' or '}' in object");
        }
    }

    private List<Object> readArray() {
        List<Object> array = new ArrayList<Object>();
        position++; // consume '['
        skipWhitespace();
        if (peek() == ']') {
            position++;
            return array;
        }
        while (true) {
            skipWhitespace();
            array.add(readValue());
            skipWhitespace();
            char next = peek();
            if (next == ',') {
                position++;
                continue;
            }
            if (next == ']') {
                position++;
                return array;
            }
            throw error("expected ',' or ']' in array");
        }
    }

    private String readString() {
        StringBuilder builder = new StringBuilder();
        position++; // consume opening quote
        while (true) {
            if (position >= text.length()) {
                throw error("unterminated string");
            }
            char ch = text.charAt(position++);
            if (ch == '"') {
                return builder.toString();
            }
            if (ch != '\\') {
                builder.append(ch);
                continue;
            }
            if (position >= text.length()) {
                throw error("unterminated escape sequence");
            }
            char escape = text.charAt(position++);
            switch (escape) {
                case '"':
                    builder.append('"');
                    break;
                case '\\':
                    builder.append('\\');
                    break;
                case '/':
                    builder.append('/');
                    break;
                case 'n':
                    builder.append('\n');
                    break;
                case 't':
                    builder.append('\t');
                    break;
                case 'r':
                    builder.append('\r');
                    break;
                case 'b':
                    builder.append('\b');
                    break;
                case 'f':
                    builder.append('\f');
                    break;
                case 'u':
                    if (position + 4 > text.length()) {
                        throw error("truncated unicode escape");
                    }
                    builder.append((char) Integer.parseInt(text.substring(position, position + 4), 16));
                    position += 4;
                    break;
                default:
                    throw error("unsupported escape '\\" + escape + "'");
            }
        }
    }

    private Object readNumber() {
        int start = position;
        boolean isDouble = false;
        if (peek() == '-' || peek() == '+') {
            position++;
        }
        while (position < text.length()) {
            char ch = text.charAt(position);
            if (ch >= '0' && ch <= '9') {
                position++;
            } else if (ch == '.' || ch == 'e' || ch == 'E' || ch == '-' || ch == '+') {
                // '-'/'+' only continue a number right after an exponent.
                if ((ch == '-' || ch == '+')
                        && position > start
                        && text.charAt(position - 1) != 'e'
                        && text.charAt(position - 1) != 'E') {
                    break;
                }
                if (ch == '.' || ch == 'e' || ch == 'E') {
                    isDouble = true;
                }
                position++;
            } else {
                break;
            }
        }
        String token = text.substring(start, position);
        if (token.isEmpty() || "-".equals(token) || "+".equals(token)) {
            throw error("invalid number");
        }
        try {
            if (isDouble) {
                return Double.valueOf(token);
            }
            return Long.valueOf(token);
        } catch (NumberFormatException exception) {
            throw error("invalid number '" + token + "'");
        }
    }

    private void expect(String literal) {
        if (!text.startsWith(literal, position)) {
            throw error("expected '" + literal + "'");
        }
        position += literal.length();
    }

    private char peek() {
        if (position >= text.length()) {
            throw error("unexpected end of document");
        }
        return text.charAt(position);
    }

    private void skipWhitespace() {
        while (position < text.length()) {
            char ch = text.charAt(position);
            if (ch == ' ' || ch == '\n' || ch == '\r' || ch == '\t') {
                position++;
            } else {
                break;
            }
        }
    }

    private AbstractDataException error(String message) {
        return new AbstractDataException("invalid JSON at offset " + position + ": " + message);
    }
}
