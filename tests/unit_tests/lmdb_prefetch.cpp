// Copyright (c) 2026, The Monero Project
//
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without modification, are
// permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of
//    conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list
//    of conditions and the following disclaimer in the documentation and/or other
//    materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be
//    used to endorse or promote products derived from this software without specific
//    prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
// EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL
// THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
// STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF
// THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// Tests of mdb_prefetch_overflow() (monerodato) on a small generated environment: a table of
// large values, stored by LMDB in overflow pages, as monerod stores transaction data, and a
// table of small ones. The tests read the page cache through mincore() on the environment's
// own map.

#include <gtest/gtest.h>

#include <cerrno>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <random>
#include <string>
#include <thread>
#include <vector>

#include <boost/filesystem.hpp>

#include <lmdb.h>

#if defined(__linux__)
#include <fcntl.h>
#include <sys/mman.h>
#include <unistd.h>
#endif

namespace
{
  constexpr std::size_t BLOBS = 200;
  constexpr std::size_t BLOB_SIZE = 10000; // three 4 KB overflow pages each

  struct temp_dir
  {
    boost::filesystem::path path;
    temp_dir()
      : path(boost::filesystem::temp_directory_path() / boost::filesystem::unique_path("monerodato-prefetch-%%%%-%%%%-%%%%"))
    {
      boost::filesystem::create_directories(path);
    }
    ~temp_dir()
    {
      boost::system::error_code ec;
      boost::filesystem::remove_all(path, ec);
    }
  };

  struct env_guard
  {
    MDB_env *env = nullptr;
    MDB_dbi blobs = 0, small = 0, dups = 0;
    ~env_guard()
    {
      if (env)
        mdb_env_close(env);
    }
  };

  std::string blob(uint64_t key)
  {
    std::string s(BLOB_SIZE, '\0');
    std::mt19937_64 rng(key * 1000003 + 7);
    for (std::size_t i = 0; i < s.size(); i += 8)
    {
      const uint64_t r = rng();
      memcpy(&s[i], &r, std::min<std::size_t>(8, s.size() - i));
    }
    return s;
  }

  void put(MDB_txn *txn, MDB_dbi dbi, uint64_t key, const std::string &value)
  {
    MDB_val k = {sizeof(key), &key}, v = {value.size(), (void *)value.data()};
    ASSERT_EQ(0, mdb_put(txn, dbi, &k, &v, 0));
  }

  // Open as monerod does (no read-ahead, no thread-local reader slots), with synced commits, and fill the tables
  void make_env(env_guard &g, const std::string &dir)
  {
    ASSERT_EQ(0, mdb_env_create(&g.env));
    ASSERT_EQ(0, mdb_env_set_maxdbs(g.env, 4));
    ASSERT_EQ(0, mdb_env_set_mapsize(g.env, std::size_t(64) << 20));
    ASSERT_EQ(0, mdb_env_open(g.env, dir.c_str(), MDB_NORDAHEAD | MDB_NOTLS, 0644));
    MDB_txn *txn;
    ASSERT_EQ(0, mdb_txn_begin(g.env, nullptr, 0, &txn));
    ASSERT_EQ(0, mdb_dbi_open(txn, "blobs", MDB_INTEGERKEY | MDB_CREATE, &g.blobs));
    ASSERT_EQ(0, mdb_dbi_open(txn, "small", MDB_INTEGERKEY | MDB_CREATE, &g.small));
    ASSERT_EQ(0, mdb_dbi_open(txn, "dups", MDB_INTEGERKEY | MDB_CREATE | MDB_DUPSORT, &g.dups));
    for (uint64_t key = 0; key < BLOBS; ++key)
    {
      put(txn, g.blobs, key, blob(key));
      put(txn, g.small, key, std::string(16, 's'));
      put(txn, g.dups, key, std::string(16, 'd'));
    }
    ASSERT_EQ(0, mdb_txn_commit(txn));
  }

#if defined(__linux__)
  const std::size_t os_page = (std::size_t)sysconf(_SC_PAGESIZE);

  // The first page of each value's overflow run, as an address in the environment's map
  std::vector<char *> first_pages(MDB_txn *txn, MDB_dbi dbi, uint64_t from, uint64_t step)
  {
    std::vector<char *> out;
    for (uint64_t key = from; key < BLOBS; key += step)
    {
      MDB_val k = {sizeof(key), &key}, v;
      if (mdb_get(txn, dbi, &k, &v))
        continue;
      out.push_back((char *)((uintptr_t)v.mv_data & ~(uintptr_t)(os_page - 1)));
    }
    return out;
  }

  std::size_t resident(const std::vector<char *> &pages)
  {
    std::size_t n = 0;
    for (char *p : pages)
    {
      unsigned char c = 0;
      if (mincore(p, os_page, &c) != 0)
        return SIZE_MAX;
      n += c & 1;
    }
    return n;
  }

  // Drop the whole file from the page cache; false where it cannot be done or seen (tmpfs, page size)
  bool drop_all(env_guard &g, const std::vector<char *> &probe)
  {
    MDB_txn *txn;
    MDB_stat st;
    if (mdb_txn_begin(g.env, nullptr, MDB_RDONLY, &txn))
      return false;
    const int rc = mdb_stat(txn, g.blobs, &st);
    mdb_txn_abort(txn);
    mdb_filehandle_t fd;
    if (rc || st.ms_psize != os_page || mdb_env_get_fd(g.env, &fd))
      return false;
    for (char *p : probe)
      madvise(p, os_page, MADV_DONTNEED);
    posix_fadvise(fd, 0, 0, POSIX_FADV_DONTNEED);
    return resident(probe) == 0;
  }
#endif
}

TEST(lmdb_prefetch, checks_its_arguments)
{
  temp_dir dir;
  env_guard g;
  make_env(g, dir.path.string());
  MDB_txn *txn;
  ASSERT_EQ(0, mdb_txn_begin(g.env, nullptr, MDB_RDONLY, &txn));
  uint64_t key = 1, missing = BLOBS + 5;
  MDB_val k = {sizeof(key), &key}, m = {sizeof(missing), &missing};
  const int rc = mdb_prefetch_overflow(txn, g.blobs, &k);
  if (rc == ENOSYS)
  {
    mdb_txn_abort(txn);
    GTEST_SKIP() << "not available in this build";
  }
  EXPECT_EQ(0, rc);
  EXPECT_EQ(0, mdb_prefetch_overflow(txn, g.small, &k)); // a value in a leaf: nothing to do
  EXPECT_EQ(MDB_NOTFOUND, mdb_prefetch_overflow(txn, g.blobs, &m));
  EXPECT_EQ(EINVAL, mdb_prefetch_overflow(txn, g.dups, &k));
  EXPECT_EQ(EINVAL, mdb_prefetch_overflow(txn, g.blobs, nullptr));
  mdb_txn_abort(txn);

  // in a write transaction, a value it wrote itself is in memory: nothing to do, no error
  ASSERT_EQ(0, mdb_txn_begin(g.env, nullptr, 0, &txn));
  put(txn, g.blobs, key, blob(key + 1000));
  EXPECT_EQ(0, mdb_prefetch_overflow(txn, g.blobs, &k));
  MDB_val v;
  ASSERT_EQ(0, mdb_get(txn, g.blobs, &k, &v));
  const std::string expected = blob(key + 1000);
  EXPECT_EQ(0, memcmp(expected.data(), v.mv_data, v.mv_size));
  mdb_txn_abort(txn);
}

TEST(lmdb_prefetch, reads_the_first_page_of_each_run_asked_for)
{
#if !defined(__linux__)
  GTEST_SKIP() << "needs mincore and posix_fadvise";
#else
  temp_dir dir;
  env_guard g;
  make_env(g, dir.path.string());
  MDB_txn *txn;
  ASSERT_EQ(0, mdb_txn_begin(g.env, nullptr, MDB_RDONLY, &txn));
  const std::vector<char *> asked = first_pages(txn, g.blobs, 0, 2), other = first_pages(txn, g.blobs, 1, 2);
  ASSERT_EQ(BLOBS / 2, asked.size());
  std::vector<char *> all(asked);
  all.insert(all.end(), other.begin(), other.end());
  if (!drop_all(g, all))
  {
    mdb_txn_abort(txn);
    GTEST_SKIP() << "cannot drop or count the environment's pages here";
  }

  for (uint64_t key = 0; key < BLOBS; key += 2)
  {
    MDB_val k = {sizeof(key), &key};
    const int rc = mdb_prefetch_overflow(txn, g.blobs, &k);
    if (rc == ENOSYS)
    {
      mdb_txn_abort(txn);
      GTEST_SKIP() << "not available in this build";
    }
    ASSERT_EQ(0, rc);
  }
  // the reads are asynchronous: wait for them, at most 5 s
  for (int i = 0; i < 500 && resident(asked) < asked.size(); ++i)
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  EXPECT_EQ(asked.size(), resident(asked));
  EXPECT_LE(resident(other), other.size() / 10) << "of " << other.size();

  // and the values read back intact
  for (uint64_t key = 0; key < BLOBS; ++key)
  {
    MDB_val k = {sizeof(key), &key}, v;
    ASSERT_EQ(0, mdb_get(txn, g.blobs, &k, &v));
    const std::string expected = blob(key);
    ASSERT_EQ(expected.size(), v.mv_size);
    EXPECT_EQ(0, memcmp(expected.data(), v.mv_data, v.mv_size)) << key;
  }
  mdb_txn_abort(txn);
#endif
}
