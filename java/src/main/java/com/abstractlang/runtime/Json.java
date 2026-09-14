package com.abstractlang.runtime;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Strict JSON reader for compiled Abstract documents. Produces
 * {@link LinkedHashMap} for objects (key order preserved),
 * {@link ArrayList} for arrays, {@link String}, {@link Long},
 * {@link Double}, {@link Boolean} and {@code null}.
 *
 * <p>The reader is deliberately narrow: it accepts RFC 8259 and nothing
 * else, and it reports every failure — including a malformed {@code &#92;u}
 * escape and a document nested deeper than {@link #MAX_DEPTH} — as
 * {@link AbstractDataException}. No input reaches a
 * {@code StackOverflowError}, a {@code NumberFormatException} or a
 * {@code StringIndexOutOfBoundsException}: a bundle is data shipped inside an
 * application, and a reader that dies on hostile bytes with a JVM error is a
 * reader whose failures cannot be caught where they happen.
 *
 * <p>Specifically, the reader rejects:
 *
 * <ul>
 *   <li>nesting deeper than 64 objects and arrays (SPEC Appendix D.2 item 6);
 *   <li>a leading {@code +}, a leading zero such as {@code 007}, and any
 *       number missing digits around {@code .} or an exponent (item 7);
 *   <li>duplicate keys in one object (item 7);
 *   <li>lone surrogates, escaped or literal (item 7);
 *   <li>unescaped control characters inside a string;
 *   <li>trailing content after the top-level value.
 * </ul>
 */
final class Json {

    /**
     * The deepest nesting the reader will follow, matching the compiler's own
     * document depth limit (SPEC §3.7). Recursion is bounded before the stack
     * is, so a document of ten thousand open brackets is a diagnostic and not
     * a {@code StackOverflowError}.
     */
    static final int MAX_DEPTH = 64;

    private final String text;
    private int position;
    private int depth;

    private Json(String text) {
        this.text = text;
    }

    static Object parse(String text) {
        if (text == null) {
            throw new AbstractDataException("cannot parse a null JSON document");
        }
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
                if (head == '-' || (head >= '0' && head <= '9')) {
                    return readNumber();
                }
                throw error("unexpected character '" + head + "'");
        }
    }

    private void enter() {
        depth++;
        if (depth > MAX_DEPTH) {
            throw error("document nests deeper than " + MAX_DEPTH + " levels");
        }
    }

    private Map<String, Object> readObject() {
        enter();
        Map<String, Object> object = new LinkedHashMap<String, Object>();
        position++; // consume '{'
        skipWhitespace();
        if (peek() == '}') {
            position++;
            depth--;
            return object;
        }
        while (true) {
            skipWhitespace();
            if (peek() != '"') {
                throw error("expected an object key");
            }
            String key = readString();
            // A duplicate key makes a document mean two things at once; the
            // reader refuses rather than silently picking one of them.
            if (object.containsKey(key)) {
                throw error("duplicate object key '" + key + "'");
            }
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
                depth--;
                return object;
            }
            throw error("expected ',' or '}' in object");
        }
    }

    private List<Object> readArray() {
        enter();
        List<Object> array = new ArrayList<Object>();
        position++; // consume '['
        skipWhitespace();
        if (peek() == ']') {
            position++;
            depth--;
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
                depth--;
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
            if (ch < 0x20) {
                throw error("unescaped control character U+"
                        + String.format("%04X", (int) ch) + " in string");
            }
            if (ch != '\\') {
                if (Character.isSurrogate(ch)) {
                    appendSurrogatePair(builder, ch, false);
                } else {
                    builder.append(ch);
                }
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
                case 'u': {
                    char unit = readHexQuad();
                    if (Character.isSurrogate(unit)) {
                        appendSurrogatePair(builder, unit, true);
                    } else {
                        builder.append(unit);
                    }
                    break;
                }
                default:
                    throw error("unsupported escape '\\" + escape + "'");
            }
        }
    }

    /**
     * Reads the four hexadecimal digits of a {@code &#92;u} escape. A short or
     * non-hexadecimal escape is this runtime's own exception, never a
     * {@code NumberFormatException} from {@code Integer.parseInt} (SPEC
     * Appendix D.2 item 8).
     */
    private char readHexQuad() {
        if (position + 4 > text.length()) {
            throw error("truncated unicode escape");
        }
        int value = 0;
        for (int index = 0; index < 4; index++) {
            char digit = text.charAt(position + index);
            int nibble = hexValue(digit);
            if (nibble < 0) {
                throw error("malformed unicode escape '\\u"
                        + text.substring(position, position + 4) + "'");
            }
            value = (value << 4) | nibble;
        }
        position += 4;
        return (char) value;
    }

    private static int hexValue(char digit) {
        if (digit >= '0' && digit <= '9') {
            return digit - '0';
        }
        if (digit >= 'a' && digit <= 'f') {
            return digit - 'a' + 10;
        }
        if (digit >= 'A' && digit <= 'F') {
            return digit - 'A' + 10;
        }
        return -1;
    }

    /**
     * Appends a surrogate pair, refusing a lone surrogate. {@code high} is the
     * unit just read; {@code escaped} says whether it came from a
     * {@code &#92;u} escape, which is also how its partner must be written.
     */
    private void appendSurrogatePair(StringBuilder builder, char high, boolean escaped) {
        if (!Character.isHighSurrogate(high)) {
            throw error("lone low surrogate U+" + String.format("%04X", (int) high));
        }
        char low;
        if (escaped) {
            if (position + 1 >= text.length()
                    || text.charAt(position) != '\\'
                    || text.charAt(position + 1) != 'u') {
                throw error("lone high surrogate U+" + String.format("%04X", (int) high));
            }
            position += 2;
            low = readHexQuad();
        } else {
            if (position >= text.length()) {
                throw error("lone high surrogate U+" + String.format("%04X", (int) high));
            }
            low = text.charAt(position++);
        }
        if (!Character.isLowSurrogate(low)) {
            throw error("high surrogate U+" + String.format("%04X", (int) high)
                    + " is not followed by a low surrogate");
        }
        builder.append(high).append(low);
    }

    /**
     * Reads a number in exactly the RFC 8259 grammar:
     * {@code [-] (0 | [1-9][0-9]*) [. [0-9]+] [(e|E) [+|-] [0-9]+]}.
     *
     * <p>A leading {@code +}, a leading zero and a missing digit around
     * {@code .} or an exponent are all refused (SPEC Appendix D.2 item 7).
     * Anything with a fraction or an exponent becomes a {@link Double}; every
     * other number becomes a {@link Long}, or a {@link Double} when it does
     * not fit in 64 bits.
     */
    private Object readNumber() {
        int start = position;
        boolean isDouble = false;

        if (peek() == '-') {
            position++;
        }
        if (position >= text.length()) {
            throw error("number '" + text.substring(start) + "' is incomplete");
        }
        char first = text.charAt(position);
        if (first == '0') {
            position++;
            if (position < text.length() && isDigit(text.charAt(position))) {
                throw error("number '" + text.substring(start, position + 1)
                        + "' has a leading zero");
            }
        } else if (first >= '1' && first <= '9') {
            while (position < text.length() && isDigit(text.charAt(position))) {
                position++;
            }
        } else {
            throw error("expected a digit in a number");
        }

        if (position < text.length() && text.charAt(position) == '.') {
            isDouble = true;
            position++;
            if (position >= text.length() || !isDigit(text.charAt(position))) {
                throw error("number has no digits after '.'");
            }
            while (position < text.length() && isDigit(text.charAt(position))) {
                position++;
            }
        }

        if (position < text.length()
                && (text.charAt(position) == 'e' || text.charAt(position) == 'E')) {
            isDouble = true;
            position++;
            if (position < text.length()
                    && (text.charAt(position) == '+' || text.charAt(position) == '-')) {
                position++;
            }
            if (position >= text.length() || !isDigit(text.charAt(position))) {
                throw error("number has no digits in its exponent");
            }
            while (position < text.length() && isDigit(text.charAt(position))) {
                position++;
            }
        }

        String token = text.substring(start, position);
        if (isDouble) {
            return Double.valueOf(parseDouble(token));
        }
        try {
            return Long.valueOf(token);
        } catch (NumberFormatException exception) {
            // An integer literal wider than 64 bits keeps its magnitude as a
            // double rather than failing the whole document.
            return Double.valueOf(parseDouble(token));
        }
    }

    private double parseDouble(String token) {
        try {
            return Double.parseDouble(token);
        } catch (NumberFormatException exception) {
            throw error("invalid number '" + token + "'");
        }
    }

    private static boolean isDigit(char ch) {
        return ch >= '0' && ch <= '9';
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
