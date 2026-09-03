import com.abstractlang.runtime.*;
public class HexErr {
  public static void main(String[] a){
    // 64 chars but non-hex 'zz' at the start
    String bad="zz0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    System.out.println("len="+bad.length());
    try { AbstractKeys.fromHex(bad); System.out.println("no throw"); }
    catch (AbstractDataException e){ System.out.println("AbstractDataException: "+e.getMessage()); }
    catch (RuntimeException e){ System.out.println("LEAKED "+e.getClass().getName()+": "+e.getMessage()); }
    // Rust CLI equivalent: 'hex:zz...' should be a clean error
  }
}
