package com.abstractlang.runtime;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The full set of compiled instances loaded from a bundle or JSON document,
 * indexed by id and by template name.
 */
public final class AbstractData implements Iterable<AbstractObject> {

    private final List<AbstractObject> items;
    private final Map<String, AbstractObject> byId;
    private final Map<String, List<AbstractObject>> byTemplate;

    @SuppressWarnings("unchecked")
    private AbstractData(List<Object> rawItems) {
        this.items = new ArrayList<AbstractObject>();
        this.byId = new LinkedHashMap<String, AbstractObject>();
        this.byTemplate = new LinkedHashMap<String, List<AbstractObject>>();
        for (Object item : rawItems) {
            if (!(item instanceof Map)) {
                continue;
            }
            AbstractObject object = new AbstractObject((Map<String, Object>) item);
            items.add(object);
            byId.put(object.id(), object);
            List<AbstractObject> siblings = byTemplate.get(object.template());
            if (siblings == null) {
                siblings = new ArrayList<AbstractObject>();
                byTemplate.put(object.template(), siblings);
            }
            siblings.add(object);
        }
    }

    /** Parses a compiled Abstract JSON document ({@code {"data": [...]}}). */
    @SuppressWarnings("unchecked")
    public static AbstractData fromJson(String json) {
        Object root = Json.parse(json);
        if (!(root instanceof Map)) {
            throw new AbstractDataException("compiled document must be a JSON object");
        }
        Object data = ((Map<String, Object>) root).get("data");
        if (!(data instanceof List)) {
            throw new AbstractDataException("compiled document is missing the 'data' array");
        }
        return new AbstractData((List<Object>) data);
    }

    /** Every compiled instance, in project order. */
    public List<AbstractObject> all() {
        return Collections.unmodifiableList(items);
    }

    /** The instance with this id, or {@code null}. */
    public AbstractObject get(String id) {
        return byId.get(id);
    }

    /** The instance with this id, or an exception with a clear message. */
    public AbstractObject require(String id) {
        AbstractObject object = byId.get(id);
        if (object == null) {
            throw new AbstractDataException("no instance with id '" + id + "' in this bundle");
        }
        return object;
    }

    /** Every instance compiled from the given template (schema) name. */
    public List<AbstractObject> byTemplate(String template) {
        List<AbstractObject> found = byTemplate.get(template);
        return found == null ? Collections.<AbstractObject>emptyList()
                : Collections.unmodifiableList(found);
    }

    public int size() {
        return items.size();
    }

    @Override
    public Iterator<AbstractObject> iterator() {
        return all().iterator();
    }
}
