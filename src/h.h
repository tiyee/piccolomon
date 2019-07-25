#ifndef H_H
#define H_H
#include<iostream>
#include"token.h"
enum Level {
    One = 1,Two,Three,Four,Five,Six
};
class H:public Token
{
private:
    Level _level;
    std::string _txt;

public:
     H(Level lv,std::string txt,unsigned int line,unsigned int pos,unsigned int length);
     H(const H &) = delete;

     H &operator=(const H &) = delete;
     void writeAsHtml(std::ostream&) override;
     void writeToken(std::ostream& out) override;
};

#endif // H_H
