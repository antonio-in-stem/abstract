package com.abstractlang.runtime;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * One compiled Abstract document: the base instances of {@code data}, indexed
 * by id and by template name, together with the {@code overlays} that carry
 * the versions which differ from the base (SPEC §7.5, §8.1).
 *
 * <p>The base is the document for the project's <em>maximum</em> version.
 * {@link #forVersion(int)} materialises any other version:
 *
 * <pre>{@code
 * AbstractData data = AbstractBundle.loadResource(AppResources.class, "/configuration.abx", key);
 * AbstractData v2   = data.forVersion(2);
 * for (AbstractObject record : v2.byTemplate("Record")) {
 *     register(record.id(), record.getString("title"));
 * }
 * }</pre>
 */
public final class AbstractData implements Iterable<AbstractObject> {

    private final List<AbstractObject> items;
    private final Map<String, AbstractObject> byId;
    private final Map<String, List<AbstractObject>> byTemplate;
    private final List<Overlay> overlays;
    private final int minVersion;
    private final int maxVersion;

    @SuppressWarnings("unchecked")
    private AbstractData(List<Object> rawItems, List<Overlay> overlays, int min, int max) {
        this.items = new ArrayList<AbstractObject>();
        this.byId = new LinkedHashMap<String, AbstractObject>();
        this.byTemplate = new LinkedHashMap<String, List<AbstractObject>>();
        this.overlays = overlays;
        this.minVersion = min;
        this.maxVersion = max;
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

    /**
     * Parses a compiled Abstract document: the envelope of SPEC §8.1, with
     * {@code abstract}, {@code data} and {@code overlays}.
     *
     * <p>A document whose {@code abstract.format} is not 1 is refused rather
     * than read as if it were: {@code format} changes only when the shape of
     * the document changes.
     */
    @SuppressWarnings("unchecked")
    public static AbstractData fromJson(String json) {
        Object root = Json.parse(json);
        if (!(root instanceof Map)) {
            throw new AbstractDataException("compiled document must be a JSON object");
        }
        Map<String, Object> document = (Map<String, Object>) root;

        int min = 1;
        int max = 1;
        Object envelope = document.get("abstract");
        if (envelope != null) {
            if (!(envelope instanceof Map)) {
                throw new AbstractDataException("'abstract' must be an object");
            }
            Map<String, Object> meta = (Map<String, Object>) envelope;
            Object format = meta.get("format");
            if (format != null && !(format instanceof Long && ((Long) format).longValue() == 1L)) {
                throw new AbstractDataException(
                        "unsupported document format " + format + "; this runtime reads format 1");
            }
            Object versions = meta.get("versions");
            if (versions != null) {
                min = requireVersion(versions, "min");
                max = requireVersion(versions, "max");
                if (min > max) {
                    throw new AbstractDataException(
                            "version range " + min + ".." + max + " is reversed");
                }
            }
        }

        Object data = document.get("data");
        if (!(data instanceof List)) {
            throw new AbstractDataException("compiled document is missing the 'data' array");
        }
        return new AbstractData(
                (List<Object>) data, readOverlays(document.get("overlays")), min, max);
    }

    /**
     * The document for one version, built by SPEC §7.5 step 3: start from the
     * base, then apply <strong>every</strong> overlay whose range contains
     * {@code version} — there may be none, one or several — replacing or
     * adding objects by {@code id} from the overlay's {@code data} and then
     * deleting the ids in its {@code removed}.
     *
     * <p>Two assumptions this method does not make, because SPEC §7.5 does not
     * license either: that at most one overlay matches a version — overlay
     * ranges may overlap, and only the ranges that mention one id are
     * disjoint — and that every id in an overlay is already present in the
     * base, since an instance whose window ends before the base version
     * appears in an overlay and nowhere else.
     *
     * <p>The result is ordered canonically by {@code (template, id)} as
     * SPEC §2.7 prescribes, so a version reached through overlays is ordered
     * exactly like the base. It carries no overlays of its own: it is already
     * one version, and its range is {@code version..version}.
     *
     * @throws AbstractDataException when {@code version} lies outside the
     *     range the document declares.
     */
    public AbstractData forVersion(int version) {
        if (version < minVersion || version > maxVersion) {
            throw new AbstractDataException("version " + version + " is outside this document's"
                    + " range " + minVersion + ".." + maxVersion);
        }

        Map<String, Map<String, Object>> resolved =
                new LinkedHashMap<String, Map<String, Object>>();
        for (AbstractObject item : items) {
            resolved.put(item.id(), item.asMap());
        }
        for (Overlay overlay : overlays) {
            if (version < overlay.min || version > overlay.max) {
                continue;
            }
            for (Map<String, Object> replacement : overlay.data) {
                resolved.put(idOf(replacement), replacement);
            }
            for (String id : overlay.removed) {
                resolved.remove(id);
            }
        }

        List<Object> materialised = new ArrayList<Object>(resolved.values());
        Collections.sort(materialised, CANONICAL_ORDER);
        return new AbstractData(materialised, Collections.<Overlay>emptyList(), version, version);
    }

    /** The lowest version this document describes (SPEC §4.12). */
    public int minVersion() {
        return minVersion;
    }

    /** The highest version this document describes; the base is this one. */
    public int maxVersion() {
        return maxVersion;
    }

    /** How many overlays the document carries (SPEC §7.5). */
    public int overlayCount() {
        return overlays.size();
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

    // ------------------------------------------------------------------

    /** One overlay: a version range, replacement objects, and removed ids. */
    private static final class Overlay {
        private final int min;
        private final int max;
        private final List<Map<String, Object>> data;
        private final List<String> removed;

        private Overlay(int min, int max, List<Map<String, Object>> data, List<String> removed) {
            this.min = min;
            this.max = max;
            this.data = data;
            this.removed = removed;
        }
    }

    @SuppressWarnings("unchecked")
    private static List<Overlay> readOverlays(Object raw) {
        if (raw == null) {
            return Collections.emptyList();
        }
        if (!(raw instanceof List)) {
            throw new AbstractDataException("'overlays' must be an array");
        }
        List<Overlay> overlays = new ArrayList<Overlay>();
        for (Object entry : (List<Object>) raw) {
            if (!(entry instanceof Map)) {
                throw new AbstractDataException("every overlay must be an object");
            }
            Map<String, Object> overlay = (Map<String, Object>) entry;
            Object versions = overlay.get("versions");
            if (versions == null) {
                throw new AbstractDataException("every overlay must carry a 'versions' range");
            }
            int min = requireVersion(versions, "min");
            int max = requireVersion(versions, "max");
            if (min > max) {
                throw new AbstractDataException(
                        "overlay range " + min + ".." + max + " is reversed");
            }
            overlays.add(new Overlay(min, max, readOverlayData(overlay.get("data")),
                    readRemoved(overlay.get("removed"))));
        }
        return overlays;
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> readOverlayData(Object raw) {
        List<Map<String, Object>> data = new ArrayList<Map<String, Object>>();
        if (raw == null) {
            return data;
        }
        if (!(raw instanceof List)) {
            throw new AbstractDataException("an overlay's 'data' must be an array");
        }
        for (Object item : (List<Object>) raw) {
            if (!(item instanceof Map)) {
                throw new AbstractDataException("an overlay's 'data' must hold objects");
            }
            data.add((Map<String, Object>) item);
        }
        return data;
    }

    @SuppressWarnings("unchecked")
    private static List<String> readRemoved(Object raw) {
        List<String> removed = new ArrayList<String>();
        if (raw == null) {
            return removed;
        }
        if (!(raw instanceof List)) {
            throw new AbstractDataException("an overlay's 'removed' must be an array");
        }
        for (Object item : (List<Object>) raw) {
            if (!(item instanceof String)) {
                throw new AbstractDataException("an overlay's 'removed' must hold ids");
            }
            removed.add((String) item);
        }
        return removed;
    }

    @SuppressWarnings("unchecked")
    private static int requireVersion(Object versions, String key) {
        if (!(versions instanceof Map)) {
            throw new AbstractDataException("a version range must be an object");
        }
        Object value = ((Map<String, Object>) versions).get(key);
        if (!(value instanceof Long)) {
            throw new AbstractDataException("a version range needs an integer '" + key + "'");
        }
        long version = ((Long) value).longValue();
        if (version < 1 || version > Integer.MAX_VALUE) {
            throw new AbstractDataException("version " + version + " is out of range");
        }
        return (int) version;
    }

    private static String idOf(Map<String, Object> object) {
        Object id = object.get("id");
        if (!(id instanceof String)) {
            throw new AbstractDataException("every instance object must carry a string 'id'");
        }
        return (String) id;
    }

    private static String textOf(Map<String, Object> object, String key) {
        Object value = object.get(key);
        return value instanceof String ? (String) value : "";
    }

    /**
     * SPEC §2.7 order: by template, then by id, each compared as a sequence of
     * Unicode scalar values rather than of UTF-16 code units.
     */
    private static final Comparator<Object> CANONICAL_ORDER = new Comparator<Object>() {
        @Override
        @SuppressWarnings("unchecked")
        public int compare(Object left, Object right) {
            Map<String, Object> a = (Map<String, Object>) left;
            Map<String, Object> b = (Map<String, Object>) right;
            int byTemplate = compareScalarValues(textOf(a, "template"), textOf(b, "template"));
            if (byTemplate != 0) {
                return byTemplate;
            }
            return compareScalarValues(textOf(a, "id"), textOf(b, "id"));
        }
    };

    private static int compareScalarValues(String left, String right) {
        int leftIndex = 0;
        int rightIndex = 0;
        while (leftIndex < left.length() && rightIndex < right.length()) {
            int leftPoint = left.codePointAt(leftIndex);
            int rightPoint = right.codePointAt(rightIndex);
            if (leftPoint != rightPoint) {
                return leftPoint < rightPoint ? -1 : 1;
            }
            leftIndex += Character.charCount(leftPoint);
            rightIndex += Character.charCount(rightPoint);
        }
        return (left.length() - leftIndex) - (right.length() - rightIndex);
    }
}
