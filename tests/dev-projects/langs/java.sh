#!/bin/sh
# Minimal Java project: compile and run.

. "$(dirname "$0")/../common.sh"

# Note: Arch Linux uses jdk-openjdk, Debian/Ubuntu uses openjdk-17-jdk, Alpine uses openjdk17, openEuler uses java-11-openjdk-devel
# openEuler has xorg-x11-fonts packaging conflict, need --ignore-file-conflicts
# Brew: openjdk needs gcc for libstdc++.so dependency
if [ "$OS" = "openeuler" ]; then
    $EPKG_BIN -e "$ENV_NAME" --assume-yes install --ignore-missing --ignore-file-conflicts java-1.8.0-openjdk-devel || true
fi
if [ "$OS" = "brew" ]; then
    $EPKG_BIN -e "$ENV_NAME" --assume-yes install --ignore-missing gcc || true
fi
run_install java-1.8.0-openjdk-devel openjdk-17-jdk default-jdk java-openjdk openjdk17-jre openjdk17 openjdk-17 openjdk jdk-openjdk jdk17-openjdk java-11-openjdk-devel java-25-openjdk-devel
check_cmd javac -version || lang_skip "no java for OS=$OS"

run_ebin javac -version
run_ebin_if java -version

# Create test file - use java for conda/msys2 (no /bin/sh)
if [ "$OS" = "conda" ] || [ "$OS" = "msys2" ]; then
    run java -e "
        import java.io.*;
        public class init {
            public static void main(String[] args) throws Exception {
                new File(\"$TEST_TMP/javaproj\").mkdirs();
                try (PrintWriter w = new PrintWriter(\"$TEST_TMP/javaproj/Main.java\")) {
                    w.println(\"public class Main { public static void main(String[] args) { System.out.println(\"ok\"); } }\");
                }
            }
        }
    " 2>/dev/null || run java -e "new java.io.File(\"$TEST_TMP/javaproj\").mkdirs(); try (var w = new java.io.PrintWriter(\"$TEST_TMP/javaproj/Main.java\")) { w.println(\"public class Main { public static void main(String[] args) { System.out.println(\"ok\"); } }\"); }"
    run javac "$TEST_TMP/javaproj/Main.java"
    run java -cp "$TEST_TMP/javaproj" Main | grep -q ok
    lang_ok
    exit 0
fi

run /bin/sh -c "mkdir -p $TEST_TMP/javaproj && cd $TEST_TMP/javaproj && printf '%s\n' 'public class Main { public static void main(String[] args) { System.out.println(\"ok\"); } }' > Main.java"
run /bin/sh -c "cd $TEST_TMP/javaproj && javac Main.java && java Main" | grep -q ok
lang_ok
