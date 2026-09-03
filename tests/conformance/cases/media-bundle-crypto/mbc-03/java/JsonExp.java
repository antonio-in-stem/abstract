import com.abstractlang.runtime.*;
import java.lang.reflect.*;

public class JsonExp {
    static Object parse(String s) throws Exception {
        Class<?> c = Class.forName("com.abstractlang.runtime.Json");
        Method m = c.getDeclaredMethod("parse", String.class);
        m.setAccessible(true);
        return m.invoke(null, s);
    }
    public static void main(String[] a) throws Exception {
        // 1. Deep nesting -> StackOverflowError (uncaught)
        int depth = 60000;
        StringBuilder sb = new StringBuilder();
        for (int i=0;i<depth;i++) sb.append('[');
        for (int i=0;i<depth;i++) sb.append(']');
        try {
            parse(sb.toString());
            System.out.println("deep-nesting: parsed OK (no limit)");
        } catch (InvocationTargetException e) {
            System.out.println("deep-nesting throws: " + e.getCause().getClass().getName());
        } catch (StackOverflowError e) {
            System.out.println("deep-nesting throws: StackOverflowError (direct)");
        }
        // 2. Duplicate keys
        try {
            Object o = parse("{\"a\":1,\"b\":2,\"a\":3}");
            System.out.println("dup-keys: " + o);
        } catch (Throwable t) { System.out.println("dup-keys err: "+t); }
        // 3. Lone surrogate -> UTF-8 re-encode replaces with '?'
        Object s = parse("\"\ud83d\"");
        byte[] bytes = ((String)s).getBytes("UTF-8");
        StringBuilder hx = new StringBuilder();
        for (byte b: bytes) hx.append(String.format("%02x ", b));
        System.out.println("lone-surrogate UTF-8 bytes: " + hx.toString().trim());
        // 4. Leading + and leading zeros
        System.out.println("num +5 -> " + parse("+5") + " (" + parse("+5").getClass().getSimpleName()+")");
        System.out.println("num 007 -> " + parse("007"));
        System.out.println("num 01.5 -> " + parse("01.5"));
    }
}
