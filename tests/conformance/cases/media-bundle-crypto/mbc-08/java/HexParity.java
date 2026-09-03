import com.abstractlang.runtime.*;
import java.nio.file.*;

public class HexParity {
    public static void main(String[] a) throws Exception {
        String K = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        byte[] bundle = Files.readAllBytes(Paths.get(a[0]));
        System.out.println("AbstractKeys.fromHex accepts the bare string: "
            + (AbstractKeys.fromHex(K).length) + " bytes");
        try {
            AbstractData data = AbstractBundle.load(bundle, AbstractKeys.fromHex(K));
            System.out.println("opened with fromHex -> " + data.size() + " item(s)");
        } catch (AbstractDataException e) {
            System.out.println("fromHex(K) FAILED: " + e.getMessage());
        }
        AbstractData ok = AbstractBundle.load(bundle, AbstractKeys.fromPassphrase(K));
        System.out.println("fromPassphrase(K) opened -> " + ok.size() + " item(s), first="
            + ok.all().get(0).getString("name"));
    }
}
