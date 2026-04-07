# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.kts.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# Keep accessibility service
-keep class com.smsreader.service.SmsAccessibilityService { *; }

# Keep model classes
-keep class com.smsreader.model.** { *; }

# Keep network classes
-keep class com.smsreader.network.** { *; }