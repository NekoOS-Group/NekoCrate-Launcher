// Generate conformance results using the supplied, unmodified Fabric Loader JAR.
// Input: base64(predicate) TAB base64(version); output: T/F/E per line.
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import net.fabricmc.loader.api.Version;
import net.fabricmc.loader.api.metadata.version.VersionPredicate;
class FabricVersionOracle {
    public static void main(String[] args) throws Exception {
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        for (String line; (line = reader.readLine()) != null;) {
            String[] fields = line.split("\t", -1);
            String predicate = new String(Base64.getDecoder().decode(fields[0]), StandardCharsets.UTF_8);
            String version = new String(Base64.getDecoder().decode(fields[1]), StandardCharsets.UTF_8);
            try {
                System.out.println(VersionPredicate.parse(predicate).test(Version.parse(version)) ? "T" : "F");
            } catch (Exception error) {
                System.out.println("E");
            }
        }
    }
}
