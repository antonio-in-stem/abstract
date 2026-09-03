import com.abstractlang.runtime.*;
import java.nio.file.*;
public class Downgrade {
  public static void main(String[] a) throws Exception {
    byte[] ff = Files.readAllBytes(Paths.get("flagflipped.abx"));
    // openPayload with null key: downgrade NOT refused, returns raw ciphertext bytes
    byte[] p = AbstractBundle.openPayload(ff, null);
    System.out.println("openPayload(null) returned " + p.length + " bytes (raw ciphertext, no error)");
    System.out.print("first bytes hex: ");
    for (int i=0;i<Math.min(12,p.length);i++) System.out.printf("%02x ", p[i]&0xff);
    System.out.println();
    // openPayload WITH key: refuses downgrade
    try {
      AbstractBundle.openPayload(ff, AbstractKeys.fromPassphrase("s3cret"));
      System.out.println("with key: opened?!");
    } catch (AbstractDataException e) {
      System.out.println("with key -> " + e.getMessage());
    }
  }
}
