package com.abstractlang.runtime;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;

/**
 * One compiled Abstract instance with typed, dotted-path access.
 *
 * <pre>{@code
 * AbstractObject sticker = data.get("golden_dragon");
 * String rarity   = sticker.getString("rarity");
 * long variants   = sticker.getInt("variant_count", 0);
 * boolean shiny   = sticker.getBool("featured", false);
 * String slotMode = sticker.getString("slots.1.mode");
 * for (AbstractObject entry : sticker.getObjects("lang_values")) {
 *     register(entry.getString("key"), entry.getString("value"));
 * }
 * }</pre>
 */
public final class AbstractObject {

    private final Map<String, Object> fields;

    AbstractObject(Map<String, Object> fields) {
        this.fields = fields;
    }

    /** The instance id (every compiled instance carries one). */
    public String id() {
        return getString("id", "");
    }

    /** The template (schema) name this instance was validated against. */
    public String template() {
        return getString("template", "");
    }

    /** True when a dotted path resolves to a value. */
    public boolean has(String path) {
        return resolve(path) != null;
    }

    /** The raw value at a dotted path, or {@code null}. */
    public Object raw(String path) {
        return resolve(path);
    }

    public String getString(String path) {
        Object value = require(path);
        return stringify(value);
    }

    public String getString(String path, String fallback) {
        Object value = resolve(path);
        return value == null ? fallback : stringify(value);
    }

    public long getInt(String path) {
        return toLong(require(path), path);
    }

    public long getInt(String path, long fallback) {
        Object value = resolve(path);
        return value == null ? fallback : toLong(value, path);
    }

    public double getFloat(String path) {
        return toDouble(require(path), path);
    }

    public double getFloat(String path, double fallback) {
        Object value = resolve(path);
        return value == null ? fallback : toDouble(value, path);
    }

    public boolean getBool(String path) {
        return toBool(require(path), path);
    }

    public boolean getBool(String path, boolean fallback) {
        Object value = resolve(path);
        return value == null ? fallback : toBool(value, path);
    }

    /** A nested object, or {@code null} when the path is absent. */
    @SuppressWarnings("unchecked")
    public AbstractObject getObject(String path) {
        Object value = resolve(path);
        if (value == null) {
            return null;
        }
        if (value instanceof Map) {
            return new AbstractObject((Map<String, Object>) value);
        }
        throw new AbstractDataException("value at '" + path + "' is not an object");
    }

    /** The raw list at a path; empty when absent. */
    @SuppressWarnings("unchecked")
    public List<Object> getList(String path) {
        Object value = resolve(path);
        if (value == null) {
            return Collections.emptyList();
        }
        if (value instanceof List) {
            return (List<Object>) value;
        }
        return Collections.singletonList(value);
    }

    /** A list of nested objects (for tagged group lists). */
    @SuppressWarnings("unchecked")
    public List<AbstractObject> getObjects(String path) {
        List<AbstractObject> output = new ArrayList<AbstractObject>();
        for (Object item : getList(path)) {
            if (item instanceof Map) {
                output.add(new AbstractObject((Map<String, Object>) item));
            }
        }
        return output;
    }

    /** A list of strings (numbers and booleans are stringified). */
    public List<String> getStrings(String path) {
        List<String> output = new ArrayList<String>();
        for (Object item : getList(path)) {
            output.add(stringify(item));
        }
        return output;
    }

    /** The underlying field map (mutations are discouraged). */
    public Map<String, Object> asMap() {
        return fields;
    }

    @Override
    public String toString() {
        return "AbstractObject(" + template() + ":" + id() + ")";
    }

    // ------------------------------------------------------------------

    @SuppressWarnings("unchecked")
    private Object resolve(String path) {
        Object current = fields;
        for (String segment : path.split("\\.")) {
            if (!(current instanceof Map)) {
                return null;
            }
            current = ((Map<String, Object>) current).get(segment);
            if (current == null) {
                return null;
            }
        }
        return current;
    }

    private Object require(String path) {
        Object value = resolve(path);
        if (value == null) {
            throw new AbstractDataException(
                    "missing value at '" + path + "' on instance '" + id() + "'");
        }
        return value;
    }

    private static String stringify(Object value) {
        if (value instanceof String) {
            return (String) value;
        }
        return String.valueOf(value);
    }

    private static long toLong(Object value, String path) {
        if (value instanceof Long) {
            return (Long) value;
        }
        if (value instanceof Double) {
            double number = (Double) value;
            if (number == Math.floor(number) && !Double.isInfinite(number)) {
                return (long) number;
            }
        }
        if (value instanceof String) {
            try {
                return Long.parseLong((String) value);
            } catch (NumberFormatException ignored) {
                // fall through to the error below
            }
        }
        throw new AbstractDataException("value at '" + path + "' is not an integer: " + value);
    }

    private static double toDouble(Object value, String path) {
        if (value instanceof Double) {
            return (Double) value;
        }
        if (value instanceof Long) {
            return (Long) value;
        }
        if (value instanceof String) {
            try {
                return Double.parseDouble((String) value);
            } catch (NumberFormatException ignored) {
                // fall through to the error below
            }
        }
        throw new AbstractDataException("value at '" + path + "' is not a number: " + value);
    }

    private static boolean toBool(Object value, String path) {
        if (value instanceof Boolean) {
            return (Boolean) value;
        }
        if (value instanceof String) {
            if ("true".equals(value)) {
                return true;
            }
            if ("false".equals(value)) {
                return false;
            }
        }
        throw new AbstractDataException("value at '" + path + "' is not a bool: " + value);
    }
}
