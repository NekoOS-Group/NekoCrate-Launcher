// Run with a stock Forge 1.12.2 loader + its Guava library on the classpath.
// stdin: range TAB actual version; stdout: T/F/E. Only numeric hard ranges are
// used by the launcher's conservative audit; qualifiers remain unknown.
import java.io.*;
import net.minecraftforge.fml.common.versioning.VersionRange;
import net.minecraftforge.fml.common.versioning.DefaultArtifactVersion;
class MavenNumericOracle {
    public static void main(String[] args) throws Exception {
        BufferedReader input = new BufferedReader(new InputStreamReader(System.in));
        for(String line; (line = input.readLine()) != null;) {
            String[] parts = line.split("\t", -1);
            try {
                System.out.println(VersionRange.createFromVersionSpec(parts[0]).containsVersion(new DefaultArtifactVersion(parts[1])) ? "T" : "F");
            } catch(Exception error) { System.out.println("E"); }
        }
    }
}
