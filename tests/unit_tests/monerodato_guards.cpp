// monerodato (fork-only): tests of the startup safeguard's decision. Same license as the rest of this repository (see
// LICENSE).

#include "gtest/gtest.h"

#include "common/monerodato_guards.h"

using namespace monerodato;

TEST(monerodato_guards, shipped_constant)
{
  // The release must be built with the safeguard on. A test build changes TEST_BUILD only, by editing the header.
  EXPECT_FALSE(TEST_BUILD);
}

TEST(monerodato_guards, network_shipped)
{
  for (const program p : {program::daemon, program::wallet, program::utility})
  {
    EXPECT_TRUE(network_allowed(cryptonote::TESTNET, p, false));
    EXPECT_FALSE(network_allowed(cryptonote::MAINNET, p, false));
    EXPECT_FALSE(network_allowed(cryptonote::STAGENET, p, false));
    EXPECT_FALSE(network_allowed(cryptonote::FAKECHAIN, p, false));
  }
}

TEST(monerodato_guards, network_test_build)
{
  for (const program p : {program::daemon, program::wallet, program::utility})
  {
    EXPECT_TRUE(network_allowed(cryptonote::TESTNET, p, true));
    EXPECT_TRUE(network_allowed(cryptonote::FAKECHAIN, p, true));
    EXPECT_FALSE(network_allowed(cryptonote::STAGENET, p, true));
  }
  // the functional tests run wallets with the mainnet network type against regtest daemons
  EXPECT_TRUE(network_allowed(cryptonote::MAINNET, program::wallet, true));
  EXPECT_FALSE(network_allowed(cryptonote::MAINNET, program::daemon, true));
  EXPECT_FALSE(network_allowed(cryptonote::MAINNET, program::utility, true));
}

TEST(monerodato_guards, refusal_exits_with_status_1)
{
  // gtest's default death-test style forks without exec. Earlier tests in this binary start threads (the compute
  // threadpool), and in the forked child the exit handlers then wait for those threads forever. The threadsafe style
  // runs this test alone in a fresh process.
  ::testing::FLAGS_gtest_death_test_style = "threadsafe";
  // the shipped build: every program refuses mainnet, stagenet and regtest with exit status 1, and returns on testnet
  for (const program p : {program::daemon, program::wallet, program::utility})
  {
    EXPECT_EXIT(enforce_network(cryptonote::MAINNET, p, "test"), ::testing::ExitedWithCode(1), "will not start test on mainnet");
    EXPECT_EXIT(enforce_network(cryptonote::STAGENET, p, "test"), ::testing::ExitedWithCode(1), "will not start test on stagenet");
    EXPECT_EXIT(enforce_network(cryptonote::FAKECHAIN, p, "test"), ::testing::ExitedWithCode(1), "will not start test on regtest");
    enforce_network(cryptonote::TESTNET, p, "test");
  }
}
